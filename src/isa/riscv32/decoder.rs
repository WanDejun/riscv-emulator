use std::collections::HashMap;

use crate::{config::arch_config::WordType, isa::riscv32::instr::*};

#[derive(Debug, Clone)]
enum PartialDecode {
    Complete(Riscv32Instr, InstrFormat),
    RequireF3,
    RequireF7,
}

pub struct Decoder {
    decode_table: HashMap<u8, PartialDecode>,
    decode_table_f3: HashMap<(u8, u8), (Riscv32Instr, InstrFormat)>,
    decode_table_f7: HashMap<(u8, u8, u8), (Riscv32Instr, InstrFormat)>,
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

impl Decoder {
    /// Build a new decoder with RV32I by default
    pub fn new() -> Self {
        Decoder::from(TABLE_RV32I.iter().cloned())
    }

    pub fn from<I>(instrs: I) -> Self
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

    pub fn decode(&self, instr: u32) -> Result<(Riscv32Instr, RVInstrInfo), Exception> {
        let opcode = (instr & 0b1111111) as u8;
        let funct3 = ((instr >> 12) & 0b111) as u8;
        let funct7 = (instr >> 25) as u8;

        let partial = self
            .decode_table
            .get(&opcode)
            .ok_or(Exception::InvalidInstruction)?
            .clone();

        match partial {
            PartialDecode::Complete(instr_kind, fmt) => {
                return Ok((instr_kind, decode_info(instr, fmt)));
            }
            PartialDecode::RequireF3 => {
                let (instr_kind, fmt) = self
                    .decode_table_f3
                    .get(&(opcode, funct3))
                    .ok_or(Exception::InvalidInstruction)?
                    .clone();
                return Ok((instr_kind, decode_info(instr, fmt)));
            }
            PartialDecode::RequireF7 => {
                let (instr_kind, fmt) = self
                    .decode_table_f7
                    .get(&(opcode, funct3, funct7))
                    .ok_or(Exception::InvalidInstruction)?
                    .clone();

                return Ok((instr_kind, decode_info(instr, fmt)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{isa::riscv32::executor::negative_of, utils::TruncateTo};

    use super::*;
    use rand::{Rng, rngs::ThreadRng};

    // TODO: add more tests

    fn get_instr_r(opcode: u8, funct3: u8, funct7: u8, rd: u8, rs1: u8, rs2: u8) -> u32 {
        (opcode as u32)
            | ((rd as u32) << 7)
            | ((funct3 as u32) << 12)
            | ((rs1 as u32) << 15)
            | ((rs2 as u32) << 20)
            | ((funct7 as u32) << 25)
    }

    fn get_instr_i(opcode: u8, funct3: u8, rd: u8, rs1: u8, imm: u32) -> u32 {
        (opcode as u32)
            | ((rd as u32) << 7)
            | ((funct3 as u32) << 12)
            | ((rs1 as u32) << 15)
            | (imm << 20)
    }

    fn get_instr_s(opcode: u8, funct3: u8, rs1: u8, rs2: u8, imm: u32) -> u32 {
        (opcode as u32)
            | ((imm & 0b11111) << 7)
            | ((funct3 as u32) << 12)
            | ((rs1 as u32) << 15)
            | ((rs2 as u32) << 20)
            | (((imm >> 5) & 0b111111) << 25)
    }

    fn get_instr_b(opcode: u8, funct3: u8, rs1: u8, rs2: u8, imm: u32) -> u32 {
        (opcode as u32)
            | ((imm >> 11) & 1) << 7
            | ((imm >> 1) & 0b1111) << 8
            | ((funct3 as u32) << 12)
            | ((rs1 as u32) << 15)
            | ((rs2 as u32) << 20)
            | ((imm >> 5) & 0x3F) << 25
            | ((imm >> 12) & 1) << 31
    }

    fn get_instr_u(opcode: u8, rd: u8, imm: u32) -> u32 {
        (opcode as u32) | ((rd as u32) << 7) | ((imm >> 12) << 12)
    }

    fn get_instr_j(opcode: u8, rd: u8, imm: u32) -> u32 {
        (opcode as u32)
            | ((rd as u32) << 7)
            | (((imm >> 12) & 0xFF) << 12)
            | (((imm >> 11) & 1) << 20)
            | (((imm >> 1) & 0x3FF) << 21)
            | (((imm >> 20) & 1) << 31)
    }

    struct Checker {
        decoder: Decoder,
        rng: ThreadRng, // TODO: consider make it fixed by seed
    }

    impl Checker {
        fn new() -> Self {
            Checker {
                decoder: Decoder::new(),
                rng: rand::rng(),
            }
        }

        fn check(&mut self, instr: u32, expected: Riscv32Instr, expected_info: RVInstrInfo) {
            let result = self.decoder.decode(instr).unwrap();
            assert_eq!(result, (expected, expected_info));
        }

        fn test_instr_r(&mut self, instr_kind: Riscv32Instr, opcode: u8, funct3: u8, funct7: u8) {
            let rd = self.rng.random_range(0..=0b11111) as u8;
            let rs1 = self.rng.random_range(0..=0b11111) as u8;
            let rs2 = self.rng.random_range(0..=0b11111) as u8;

            let instr = get_instr_r(opcode, funct3, funct7, rd, rs1, rs2);
            self.check(
                instr,
                instr_kind,
                RVInstrInfo::R {
                    rs1: rs1,
                    rs2: rs2,
                    rd: rd,
                },
            );
        }

        fn test_instr_i(&mut self, instr_kind: Riscv32Instr, opcode: u8, funct3: u8) {
            let rd = self.rng.random_range(0..=0b11111) as u8;
            let rs1 = self.rng.random_range(0..=0b11111) as u8;
            let imm = self.rng.random_range(0..=0xFFF) as u32;

            let instr = get_instr_i(opcode, funct3, rd, rs1, imm);
            self.check(
                instr,
                instr_kind,
                RVInstrInfo::I {
                    rs1: rs1,
                    rd: rd,
                    imm: imm as WordType,
                },
            );
        }

        fn test_instr_s(&mut self, instr_kind: Riscv32Instr, opcode: u8, funct3: u8) {
            let rs1 = self.rng.random_range(0..=0b11111) as u8;
            let rs2 = self.rng.random_range(0..=0b11111) as u8;
            let imm = self.rng.random_range(0..=0x7FF) as u32;

            let instr = get_instr_s(opcode, funct3, rs1, rs2, imm);
            self.check(
                instr,
                instr_kind,
                RVInstrInfo::S {
                    rs1: rs1,
                    rs2: rs2,
                    imm: imm as WordType,
                },
            );
        }

        fn test_instr_b(&mut self, instr_kind: Riscv32Instr, opcode: u8, funct3: u8) {
            let rs1 = self.rng.random_range(0..=0b11111) as u8;
            let rs2 = self.rng.random_range(0..=0b11111) as u8;
            let imm = self.rng.random_range(0..=0xFFF) << 1 as u32;

            let instr = get_instr_b(opcode, funct3, rs1, rs2, imm);
            self.check(
                instr,
                instr_kind,
                RVInstrInfo::B {
                    rs1: rs1,
                    rs2: rs2,
                    imm: imm as WordType,
                },
            );
        }

        fn test_instr_u(&mut self, instr_kind: Riscv32Instr, opcode: u8) {
            let rd = self.rng.random_range(0..=0b11111) as u8;
            let imm = self.rng.random_range(0..=0xFFFFF) << 12 as u32;

            let instr = get_instr_u(opcode, rd, imm);
            self.check(
                instr,
                instr_kind,
                RVInstrInfo::U {
                    rd: rd,
                    imm: imm as WordType,
                },
            );
        }

        fn test_instr_j(&mut self, instr_kind: Riscv32Instr, opcode: u8) {
            let rd = self.rng.random_range(0..=0b11111) as u8;
            let imm = self.rng.random_range(0..=0xFFFFF) << 1 as u32;

            let instr = get_instr_j(opcode, rd, imm);
            self.check(
                instr,
                instr_kind,
                RVInstrInfo::J {
                    rd: rd,
                    imm: imm as WordType,
                },
            );
        }
    }

    #[test]
    fn test_decoder() {
        let mut checker = Checker::new();

        for _ in 1..=1000 {
            checker.test_instr_r(Riscv32Instr::ADD, 0b0110011, 0b000, 0b0000000);
            checker.test_instr_r(Riscv32Instr::SUB, 0b0110011, 0b000, 0b0100000);

            checker.test_instr_i(Riscv32Instr::ADDI, 0b0010011, 0b000);
            checker.test_instr_i(Riscv32Instr::ORI, 0b0010011, 0b110);

            checker.test_instr_s(Riscv32Instr::SB, 0b0100011, 0b000);
            checker.test_instr_s(Riscv32Instr::SW, 0b0100011, 0b010);

            checker.test_instr_b(Riscv32Instr::BNE, 0b1100011, 0b001);

            checker.test_instr_u(Riscv32Instr::LUI, 0b0110111);

            checker.test_instr_j(Riscv32Instr::JAL, 0b1101111);
        }
    }

    #[test]
    fn test_decoder_instr() {
        let mut checker = Checker::new();

        checker.check(
            0x123450b7,
            Riscv32Instr::LUI,
            RVInstrInfo::U {
                rd: 1,
                imm: 0x12345000,
            },
        );

        checker.check(
            0x12233097,
            Riscv32Instr::AUIPC,
            RVInstrInfo::U {
                rd: 1,
                imm: 0x12233000,
            },
        );

        checker.check(
            0xffb18113,
            Riscv32Instr::ADDI,
            RVInstrInfo::I {
                rs1: 3,
                rd: 2,
                imm: negative_of(5).truncate_to(12),
            },
        );

        checker.check(
            0x00210083,
            Riscv32Instr::LB,
            RVInstrInfo::I {
                rs1: 2,
                rd: 1,
                imm: 2,
            },
        );

        checker.check(
            0xf8c318e3,
            Riscv32Instr::BNE,
            RVInstrInfo::B {
                rs1: 6,
                rs2: 12,
                imm: negative_of(112).truncate_to(13),
            },
        );
    }
}
