use std::collections::HashMap;

use crate::{
    config::arch_config::WordType,
    isa::riscv32::{
        decoder::DecoderTrait,
        instruction::{
            rv32i_table::{RV32Desc, RiscvInstr},
            *,
        },
    },
};

#[derive(Debug, Clone)]
enum PartialDecode {
    Complete(RiscvInstr, InstrFormat),
    RequireF3,
    RequireF7,
}

pub struct Decoder {
    decode_table: HashMap<u8, PartialDecode>,
    decode_table_f3: HashMap<(u8, u8), (RiscvInstr, InstrFormat)>,
    decode_table_f7: HashMap<(u8, u8, u8), (RiscvInstr, InstrFormat)>,
}

fn decode_info(instr: u32, fmt: InstrFormat) -> RVInstrInfo {
    let rd = ((instr >> 7) & 0b11111) as u8;
    let rs1 = ((instr >> 15) & 0b11111) as u8;
    let rs2 = ((instr >> 20) & 0b11111) as u8;

    match fmt {
        InstrFormat::R => RVInstrInfo::R { rd, rs1, rs2 },
        InstrFormat::I => {
            let imm = ((instr >> 20) & 0xFFF) as WordType;
            RVInstrInfo::I {
                rd: rd,
                rs1: rs1,
                imm: imm,
            }
        }
        InstrFormat::S => {
            let imm = (((instr >> 25) & 0xFF) << 5) | ((instr >> 7) & 0b11111);
            RVInstrInfo::S {
                rs1: rs1,
                rs2: rs2,
                imm: imm as WordType,
            }
        }
        InstrFormat::U => {
            let imm = (instr >> 12) << 12;
            RVInstrInfo::U {
                rd: rd,
                imm: imm as WordType,
            }
        }
        InstrFormat::B => {
            let imm = (((instr >> 31) & 1) << 12)
                | (((instr >> 7) & 1) << 11)
                | (((instr >> 25) & 0b111111) << 5)
                | (((instr >> 8) & 0b1111) << 1);
            RVInstrInfo::B {
                rs1: rs1,
                rs2: rs2,
                imm: imm as WordType,
            }
        }
        InstrFormat::J => {
            let imm = (((instr >> 31) & 1) << 20)
                | (((instr >> 12) & 0xFF) << 12)
                | (((instr >> 20) & 1) << 11)
                | (((instr >> 21) & 0x3FF) << 1);
            RVInstrInfo::J {
                rd: rd,
                imm: imm as WordType,
            }
        }
    }
}

impl DecoderTrait for Decoder {
    fn from_isa<I>(instrs: I) -> Self
    where
        I: IntoIterator<Item = RV32Desc>,
    {
        let mut decode_table = HashMap::new();
        let mut decode_table_f3 = HashMap::new();
        let mut decode_table_f7 = HashMap::new();

        for desc in instrs.into_iter() {
            let RV32Desc {
                opcode,
                funct3,
                funct7,
                instr,
                format,
            } = desc;

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
                return Some((instr_kind, decode_info(instr, fmt)));
            }
            PartialDecode::RequireF3 => {
                let (instr_kind, fmt) = self.decode_table_f3.get(&(opcode, funct3))?.clone();
                return Some((instr_kind, decode_info(instr, fmt)));
            }
            PartialDecode::RequireF7 => {
                let (instr_kind, fmt) =
                    self.decode_table_f7.get(&(opcode, funct3, funct7))?.clone();

                return Some((instr_kind, decode_info(instr, fmt)));
            }
        }
    }
}
