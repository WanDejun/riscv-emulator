use std::hint::{likely, unlikely};

use smallvec::SmallVec;

use crate::isa::riscv::{
    RiscvTypes,
    decoder::{DecodeInstr, DecoderTrait, decode_info},
    instruction::{
        instr_table::{RVInstrDesc, RiscvInstr},
        *,
    },
};

type DecoderResult = (RiscvInstr, InstrFormat);

#[derive(Debug, Clone)]
enum OpcodeDecodeResult {
    Complete(DecoderResult),
    RequireF3(Vec<Funct3DecodeResult>),
    Unknown,
}

impl OpcodeDecodeResult {
    fn insert(
        &mut self,
        funct3: Option<u8>,
        funct7: Option<u8>,
        instr: RiscvInstr,
        format: InstrFormat,
    ) {
        match self {
            Self::RequireF3(funct7_map) => {
                if let Some(funct3) = funct3 {
                    funct7_map[funct3 as usize].insert(funct7, instr, format);
                } else {
                    panic!(
                        "Two instruction with same opcode. But {:#?} is completed, others need funct3.",
                        instr
                    );
                }
            }
            Self::Unknown => {
                if let Some(funct3) = funct3 {
                    let mut inner = vec![Funct3DecodeResult::Unknown; 1 << 3];
                    inner[funct3 as usize].insert(funct7, instr, format);
                    *self = Self::RequireF3(inner);
                } else {
                    *self = Self::Complete((instr, format));
                }
            }
            Self::Complete((existed_instr, _)) => {
                if likely(funct3.is_none() && funct7.is_none()) {
                    return;
                } else {
                    panic!(
                        "Two instruction with same opcode. But {:#?} need funct3, {:#?} is completed.",
                        instr, existed_instr
                    );
                }
            }
        }
    }

    #[inline(always)]
    fn get(&self, funct3: u8, funct7: u8) -> Option<(RiscvInstr, InstrFormat)> {
        match self {
            Self::Complete(res) => Some(res.clone()),
            Self::RequireF3(funct3_map) => funct3_map[funct3 as usize].get(funct7),
            Self::Unknown => None,
        }
    }
}

#[derive(Debug, Clone)]
enum Funct3DecodeResult {
    Complete(DecoderResult),
    RequireF7(SmallMap<u8, DecoderResult>),
    Unknown,
}

impl Funct3DecodeResult {
    fn insert(&mut self, funct7: Option<u8>, instr: RiscvInstr, format: InstrFormat) {
        match self {
            Self::Complete((existed_instr, _)) => {
                if unlikely(funct7.is_some()) {
                    panic!(
                        "Two instruction with same opcode. But {:#?} need funct7, {:#?} is completed.",
                        instr, existed_instr
                    );
                }
            }
            Self::RequireF7(map) => {
                if let Some(funct7) = funct7 {
                    debug_assert!(map.get(&funct7).is_none());
                    map.insert(funct7, (instr, format));
                } else {
                    panic!(
                        "Two instruction with same opcode. But {:#?} is completed, others need funct7.",
                        instr
                    );
                }
            }
            Self::Unknown => {
                if let Some(funct7) = funct7 {
                    let mut inner = SmallMap::new();
                    inner.insert(funct7, (instr, format));
                    *self = Self::RequireF7(inner);
                } else {
                    *self = Self::Complete((instr, format));
                }
            }
        }
    }

    #[inline(always)]
    fn get(&self, funct7: u8) -> Option<(RiscvInstr, InstrFormat)> {
        match self {
            Self::Complete(res) => Some(res.clone()),
            Self::RequireF7(func7_map) => {
                let res = func7_map.get(&funct7);
                if let Some(res) = res {
                    Some(res.clone())
                } else {
                    None
                }
            }
            Self::Unknown => None,
        }
    }
}

const MAP_LENGTH: usize = 8;

#[derive(Debug, Clone)]
struct SmallMap<K, V> {
    data: SmallVec<[(K, V); MAP_LENGTH]>,
}

#[allow(unused)]
impl<K: Ord + Copy, V> SmallMap<K, V> {
    fn new() -> Self {
        SmallMap {
            data: SmallVec::new(),
        }
    }

    fn insert(&mut self, key: K, value: V) {
        self.data.push((key, value));
    }

    fn get(&self, key: &K) -> Option<&V> {
        self.data.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    fn iter(&self) -> impl Iterator<Item = &(K, V)> {
        self.data.iter()
    }

    fn len(&self) -> usize {
        self.data.len()
    }

    fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

pub struct DecodeTable(Vec<OpcodeDecodeResult>, usize);

impl DecodeTable {
    fn new() -> Self {
        Self(vec![OpcodeDecodeResult::Unknown; 1 << 7], 0)
    }

    pub fn len(&self) -> usize {
        self.1
    }

    pub fn insert(
        &mut self,
        funct3: Option<u8>,
        funct7: Option<u8>,
        opcode: u8,
        instr: RiscvInstr,
        format: InstrFormat,
    ) {
        let opcode_decode_result = &mut self.0[opcode as usize];
        opcode_decode_result.insert(funct3, funct7, instr, format);
        self.1 += 1;
    }

    pub fn get(&self, opcode: u8, funct3: u8, funct7: u8) -> Option<(RiscvInstr, InstrFormat)> {
        self.0[opcode as usize].get(funct3, funct7)
    }
}

pub(super) struct Decoder {
    decode_table: DecodeTable,
}

impl DecoderTrait<RiscvTypes> for Decoder {
    fn from_isa(instrs: &[RVInstrDesc]) -> Self {
        let mut decode_table = DecodeTable::new();

        for desc in instrs {
            if desc.use_mask {
                continue;
            }

            let RVInstrDesc {
                opcode,
                funct3,
                funct7,
                instr,
                format,
                ..
            } = desc.clone();

            match format {
                InstrFormat::R => {
                    decode_table.insert(Some(funct3), Some(funct7), opcode, instr, format);
                }
                InstrFormat::A => {
                    // rv_a instructions have only 5bits in funct7, the lower 2 bits are aq and rl.
                    // nomatter what the aq and rl bits are, the instruction is the same.
                    for i in 0..=3 {
                        let funct7_a = funct7 | i;
                        decode_table.insert(Some(funct3), Some(funct7_a), opcode, instr, format);
                    }
                }
                InstrFormat::I | InstrFormat::S | InstrFormat::B | InstrFormat::V => {
                    decode_table.insert(Some(funct3), None, opcode, instr, format);
                }

                InstrFormat::J
                | InstrFormat::U
                | InstrFormat::None
                | InstrFormat::R4_rm
                | InstrFormat::R_rm => {
                    decode_table.insert(None, None, opcode, instr, format);
                }
            }
        }

        log::info!("funct_decoder has {} instructions.", decode_table.len());

        Decoder { decode_table }
    }

    fn decode(&self, instr: u32) -> Option<DecodeInstr> {
        let opcode = (instr & 0b1111111) as u8;
        let funct3 = ((instr >> 12) & 0b111) as u8;
        let funct7 = (instr >> 25) as u8;

        let (instr_kind, fmt) = self.decode_table.get(opcode, funct3, funct7)?;

        return Some(DecodeInstr(instr_kind, decode_info(instr, instr_kind, fmt)));
    }
}
