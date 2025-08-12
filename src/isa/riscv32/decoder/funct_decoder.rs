use std::collections::HashMap;

use crate::isa::riscv32::{
    decoder::{DecoderTrait, decode_info},
    instruction::{
        rv32i_table::{RV32Desc, RiscvInstr},
        *,
    },
};

#[derive(Debug, Clone)]
enum PartialDecode {
    Complete(RiscvInstr, InstrFormat),
    RequireF3,
    RequireF7,
}

pub(super) struct Decoder {
    decode_table: HashMap<u8, PartialDecode>,
    decode_table_f3: HashMap<(u8, u8), (RiscvInstr, InstrFormat)>,
    decode_table_f7: HashMap<(u8, u8, u8), (RiscvInstr, InstrFormat)>,
}

impl DecoderTrait for Decoder {
    fn from_isa(instrs: &[RV32Desc]) -> Self {
        let mut decode_table = HashMap::new();
        let mut decode_table_f3 = HashMap::new();
        let mut decode_table_f7 = HashMap::new();

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
                    decode_table.insert(opcode, PartialDecode::RequireF7);
                    decode_table_f7.insert((opcode, funct3, funct7), (instr, format));
                }

                InstrFormat::I | InstrFormat::S | InstrFormat::B => {
                    decode_table.insert(opcode, PartialDecode::RequireF3);
                    decode_table_f3.insert((opcode, funct3), (instr, format));
                }

                _ => {
                    decode_table.insert(opcode, PartialDecode::Complete(instr, format));
                }
            }
        }

        Decoder {
            decode_table,
            decode_table_f3,
            decode_table_f7,
        }
    }

    fn decode(&self, instr: u32) -> Option<(RiscvInstr, RVInstrInfo)> {
        let opcode = (instr & 0b1111111) as u8;
        let funct3 = ((instr >> 12) & 0b111) as u8;
        let funct7 = (instr >> 25) as u8;

        let partial = self.decode_table.get(&opcode)?.clone();

        match partial {
            PartialDecode::Complete(instr_kind, fmt) => {
                return Some((instr_kind, decode_info(instr, instr_kind, fmt)));
            }
            PartialDecode::RequireF3 => {
                let (instr_kind, fmt) = self.decode_table_f3.get(&(opcode, funct3))?.clone();
                return Some((instr_kind, decode_info(instr, instr_kind, fmt)));
            }
            PartialDecode::RequireF7 => {
                let (instr_kind, fmt) =
                    self.decode_table_f7.get(&(opcode, funct3, funct7))?.clone();

                return Some((instr_kind, decode_info(instr, instr_kind, fmt)));
            }
        }
    }
}
