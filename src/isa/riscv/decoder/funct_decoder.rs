use std::hint::likely;

use smallvec::SmallVec;

use crate::isa::{
    InstrLen,
    riscv::{
        RawInstr,
        decoder::{DecodeInstr, decode_info},
        instruction::{
            instr_table::{RVInstrDesc, RiscvInstr},
            *,
        },
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

fn fix_vector_instr_decode(vector_instr: RiscvInstr, raw_vec_instr: u32) -> Option<RiscvInstr> {
    let vm = ((raw_vec_instr >> 25) & 1) != 0;
    let rs1 = (raw_vec_instr >> 15) & 0b11111;
    let rs2 = (raw_vec_instr >> 20) & 0b11111;

    let instr = match vector_instr {
        // The funct decoder keys vector arithmetic instructions by
        // opcode/funct3/funct6/vm.  Several RVV encodings use vm, rs1 or rs2 as
        // secondary opcode bits, so fix those aliases before decode_info is
        // built and execution is dispatched.
        RiscvInstr::VMADC_VV | RiscvInstr::VMADC_VVM => {
            if vm {
                RiscvInstr::VMADC_VV
            } else {
                RiscvInstr::VMADC_VVM
            }
        }
        RiscvInstr::VMSBC_VV | RiscvInstr::VMSBC_VVM => {
            if vm {
                RiscvInstr::VMSBC_VV
            } else {
                RiscvInstr::VMSBC_VVM
            }
        }
        RiscvInstr::VMADC_VX | RiscvInstr::VMADC_VXM => {
            if vm {
                RiscvInstr::VMADC_VX
            } else {
                RiscvInstr::VMADC_VXM
            }
        }
        RiscvInstr::VMSBC_VX | RiscvInstr::VMSBC_VXM => {
            if vm {
                RiscvInstr::VMSBC_VX
            } else {
                RiscvInstr::VMSBC_VXM
            }
        }
        RiscvInstr::VMADC_VI | RiscvInstr::VMADC_VIM => {
            if vm {
                RiscvInstr::VMADC_VI
            } else {
                RiscvInstr::VMADC_VIM
            }
        }

        RiscvInstr::VMERGE_VVM | RiscvInstr::VMV_V_V => {
            if vm {
                if rs2 != 0 {
                    return None;
                }
                RiscvInstr::VMV_V_V
            } else {
                RiscvInstr::VMERGE_VVM
            }
        }
        RiscvInstr::VMERGE_VXM | RiscvInstr::VMV_V_X => {
            if vm {
                if rs2 != 0 {
                    return None;
                }
                RiscvInstr::VMV_V_X
            } else {
                RiscvInstr::VMERGE_VXM
            }
        }
        RiscvInstr::VMERGE_VIM | RiscvInstr::VMV_V_I => {
            if vm {
                if rs2 != 0 {
                    return None;
                }
                RiscvInstr::VMV_V_I
            } else {
                RiscvInstr::VMERGE_VIM
            }
        }

        RiscvInstr::VMV1R_V | RiscvInstr::VMV2R_V | RiscvInstr::VMV4R_V | RiscvInstr::VMV8R_V => {
            if !vm {
                return None;
            }
            match rs1 {
                0 => RiscvInstr::VMV1R_V,
                1 => RiscvInstr::VMV2R_V,
                3 => RiscvInstr::VMV4R_V,
                7 => RiscvInstr::VMV8R_V,
                _ => return None,
            }
        }

        RiscvInstr::VZEXT_VF2
        | RiscvInstr::VZEXT_VF4
        | RiscvInstr::VZEXT_VF8
        | RiscvInstr::VSEXT_VF2
        | RiscvInstr::VSEXT_VF4
        | RiscvInstr::VSEXT_VF8 => match rs1 {
            2 => RiscvInstr::VZEXT_VF8,
            3 => RiscvInstr::VSEXT_VF8,
            4 => RiscvInstr::VZEXT_VF4,
            5 => RiscvInstr::VSEXT_VF4,
            6 => RiscvInstr::VZEXT_VF2,
            7 => RiscvInstr::VSEXT_VF2,
            _ => return None,
        },

        RiscvInstr::VCPOP_M | RiscvInstr::VFIRST_M | RiscvInstr::VMV_X_S => match rs1 {
            0 if vm => RiscvInstr::VMV_X_S,
            16 => RiscvInstr::VCPOP_M,
            17 => RiscvInstr::VFIRST_M,
            _ => return None,
        },
        RiscvInstr::VMSBF_M
        | RiscvInstr::VMSIF_M
        | RiscvInstr::VMSOF_M
        | RiscvInstr::VIOTA_M
        | RiscvInstr::VID_V => match rs1 {
            1 => RiscvInstr::VMSBF_M,
            2 => RiscvInstr::VMSOF_M,
            3 => RiscvInstr::VMSIF_M,
            16 => RiscvInstr::VIOTA_M,
            17 if rs2 == 0 => RiscvInstr::VID_V,
            _ => return None,
        },

        RiscvInstr::VADC_VVM | RiscvInstr::VADC_VXM | RiscvInstr::VADC_VIM if vm => return None,
        RiscvInstr::VSBC_VVM | RiscvInstr::VSBC_VXM if vm => return None,
        RiscvInstr::VCOMPRESS_VM if !vm => return None,
        RiscvInstr::VMV_S_X if !vm || rs2 != 0 => return None,
        RiscvInstr::VMAND_MM
        | RiscvInstr::VMNAND_MM
        | RiscvInstr::VMANDN_MM
        | RiscvInstr::VMXOR_MM
        | RiscvInstr::VMOR_MM
        | RiscvInstr::VMNOR_MM
        | RiscvInstr::VMORN_MM
        | RiscvInstr::VMXNOR_MM
            if !vm =>
        {
            return None;
        }

        _ => vector_instr,
    };

    Some(instr)
}

pub(super) struct Decoder {
    decode_table: Vec<(
        PartialDecode,
        SmallMap<(u8, u8, u8), (RiscvInstr, InstrFormat)>,
    )>,
}

impl Decoder {
    pub fn from_isa(instrs: impl Iterator<Item = RVInstrDesc>) -> Self {
        let mut decode_table = vec![(PartialDecode::Unknown, SmallMap::new()); 1 << 7];

        for desc in instrs {
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
                    let (partial, map) = &mut decode_table[opcode as usize];
                    *partial = PartialDecode::RequireF7;
                    map.insert((opcode, funct3, funct7), (instr, format));
                }
                InstrFormat::A => {
                    let (partial, map) = &mut decode_table[opcode as usize];
                    *partial = PartialDecode::RequireF7;
                    // rv_a instructions have only 5bits in funct7, the lower 2 bits are aq and rl.
                    // nomatter what the aq and rl bits are, the instruction is the same.
                    for i in 0..=3 {
                        let funct7_a = funct7 | i;
                        map.insert((opcode, funct3, funct7_a), (instr, format));
                    }
                }
                InstrFormat::V => {
                    let (partial, map) = &mut decode_table[opcode as usize];
                    *partial = PartialDecode::RequireF7;
                    let funct6_vm = funct7 | 0b01;
                    map.insert((opcode, funct3, funct6_vm), (instr, format));
                    map.insert((opcode, funct3, funct6_vm & !0b01), (instr, format));
                }
                InstrFormat::I | InstrFormat::S | InstrFormat::B => {
                    let (partial, map) = &mut decode_table[opcode as usize];
                    *partial = PartialDecode::RequireF3;
                    map.insert((opcode, funct3, 0), (instr, format));
                }

                _ => {
                    let (partial, map) = &mut decode_table[opcode as usize];
                    *partial = PartialDecode::Complete;
                    map.insert((opcode, funct3, funct7), (instr, format));
                }
            }
        }

        log::info!("funct_decoder has {} instructions.", decode_table.len());

        Decoder { decode_table }
    }

    pub fn decode(&self, instr: RawInstr) -> Option<DecodeInstr> {
        let len = instr.len();
        let instr = instr.val;

        let opcode = (instr & 0b1111111) as u8;
        let funct3 = ((instr >> 12) & 0b111) as u8;
        let funct7 = (instr >> 25) as u8;

        let (partial, map) = &self.decode_table[opcode as usize];

        let (instr_kind, fmt) = match partial {
            PartialDecode::Complete => map.data.get(0).unwrap().1.clone(),
            PartialDecode::RequireF3 => map.get(&(opcode, funct3, 0))?.clone(),
            PartialDecode::RequireF7 => map.get(&(opcode, funct3, funct7))?.clone(),
            PartialDecode::Unknown => {
                return None;
            }
        };
        let instr_kind = if likely(fmt != InstrFormat::V) {
            instr_kind
        } else {
            fix_vector_instr_decode(instr_kind, instr)?
        };

        return Some(DecodeInstr {
            instr: instr_kind,
            info: decode_info(instr, instr_kind, fmt),
            len,
        });
    }
}
