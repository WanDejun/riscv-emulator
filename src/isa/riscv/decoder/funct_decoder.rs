use smallvec::SmallVec;

use crate::isa::riscv::{
    decoder::{DecodeInstr, DecoderTrait, decode_info},
    instruction::{
        rv32i_table::{RV32Desc, RiscvInstr},
        *,
    },
};

#[derive(Debug, Clone)]
enum PartialDecode {
    Unknown,
    Complete,
    RequireF3,
    RequireF7,
}

const MAP_LENGTH: usize = 8;

#[derive(Debug, Clone)]
pub struct SmallMap<K, V> {
    data: SmallVec<[(K, V); MAP_LENGTH]>,
}

impl<K: Ord + Copy, V> SmallMap<K, V> {
    pub fn new() -> Self {
        SmallMap {
            data: SmallVec::new(),
        }
    }

    pub fn insert(&mut self, key: K, value: V) {
        self.data.push((key, value));
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.data.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn iter(&self) -> impl Iterator<Item = &(K, V)> {
        self.data.iter()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

pub(super) struct Decoder {
    decode_table: Vec<(PartialDecode, SmallMap<u32, (RiscvInstr, InstrFormat)>)>,
}

#[inline]
fn opcode_f3_f7(opcode: u8, f3: u8, f7: u8) -> u32 {
    (opcode as u32) + ((f3 as u32) << 7) + ((f7 as u32) << 10)
}

impl DecoderTrait for Decoder {
    fn from_isa(instrs: &[RV32Desc]) -> Self {
        let mut decode_table = vec![(PartialDecode::Unknown, SmallMap::new()); 1 << 7];

        for desc in instrs {
            if desc.use_mask {
                continue;
            }

            let RV32Desc {
                opcode,
                funct3,
                funct7,
                instr,
                format,
                ..
            } = desc.clone();

            match format {
                InstrFormat::R => {
                    let (partial, map) = &mut decode_table[opcode as usize];
                    *partial = PartialDecode::RequireF7;
                    map.insert(opcode_f3_f7(opcode, funct3, funct7), (instr, format));
                }

                InstrFormat::I | InstrFormat::S | InstrFormat::B => {
                    let (partial, map) = &mut decode_table[opcode as usize];
                    *partial = PartialDecode::RequireF3;
                    map.insert(opcode_f3_f7(opcode, funct3, 0), (instr, format));
                }

                _ => {
                    let (partial, map) = &mut decode_table[opcode as usize];
                    *partial = PartialDecode::Complete;
                    map.insert(opcode_f3_f7(opcode, funct3, funct7), (instr, format));
                }
            }
        }

        log::debug!("funct_decoder has {} instructions.", decode_table.len());

        Decoder { decode_table }
    }

    fn decode(&self, instr: u32) -> Option<DecodeInstr> {
        let opcode = (instr & 0b1111111) as u8;
        let funct3 = ((instr >> 12) & 0b111) as u8;
        let funct7 = (instr >> 25) as u8;

        let (partial, map) = &self.decode_table[opcode as usize];

        let (instr_kind, fmt) = match partial {
            PartialDecode::Complete => map.data.get(0).unwrap().1.clone(),
            PartialDecode::RequireF3 => map.get(&opcode_f3_f7(opcode, funct3, 0))?.clone(),
            PartialDecode::RequireF7 => map.get(&opcode_f3_f7(opcode, funct3, funct7))?.clone(),
            PartialDecode::Unknown => {
                return None;
            }
        };

        return Some(DecodeInstr(instr_kind, decode_info(instr, instr_kind, fmt)));
    }
}
