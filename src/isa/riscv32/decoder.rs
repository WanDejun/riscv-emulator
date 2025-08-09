use std::collections::HashMap;

use crate::isa::riscv32::instr::*;

#[derive(Debug, Clone)]
enum PartialDecode {
    Complete(Riscv32Instr, InstrFormat),
    RequireF3,
    RequireF7,
}

struct Decoder {
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
            let imm = ((instr >> 20) & 0xFFF) as u32;
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
                imm: imm,
            }
        }
        InstrFormat::U => {
            let imm = (instr >> 12) & 0xFFFFF;
            RVInstrInfo::U { rd: rd, imm: imm }
        }
        InstrFormat::B | InstrFormat::J => {
            todo!("I'm tired")
        }
    }
}

impl Decoder {
    pub fn new() -> Self {
        let mut decode_table = HashMap::new();
        let mut decode_table_f3 = HashMap::new();
        let mut decode_table_f7 = HashMap::new();

        for instr in TABLE_RV32I {
            let (opcode, func3, func7, instr, fmt, _) = instr.clone();
            match fmt {
                InstrFormat::R => {
                    decode_table.insert(opcode, PartialDecode::RequireF7);
                    decode_table_f7.insert((opcode, func3, func7), (instr, fmt));
                }

                InstrFormat::I | InstrFormat::S | InstrFormat::B => {
                    decode_table.insert(opcode, PartialDecode::RequireF3);
                    decode_table_f3.insert((opcode, func3), (instr, fmt));
                }

                _ => {
                    decode_table.insert(opcode, PartialDecode::Complete(instr, fmt));
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
        let func3 = ((instr >> 12) & 0b111) as u8;
        let func7 = (instr >> 25) as u8;

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
                    .get(&(opcode, func3))
                    .ok_or(Exception::InvalidInstruction)?
                    .clone();
                return Ok((instr_kind, decode_info(instr, fmt)));
            }
            PartialDecode::RequireF7 => {
                let (instr_kind, fmt) = self
                    .decode_table_f7
                    .get(&(opcode, func3, func7))
                    .ok_or(Exception::InvalidInstruction)?
                    .clone();

                return Ok((instr_kind, decode_info(instr, fmt)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{Rng, rngs::ThreadRng};

    // TODO: add more tests

    fn get_instr_r(opcode: u8, func3: u8, func7: u8, rd: u8, rs1: u8, rs2: u8) -> u32 {
        (opcode as u32)
            | ((rd as u32) << 7)
            | ((func3 as u32) << 12)
            | ((rs1 as u32) << 15)
            | ((rs2 as u32) << 20)
            | ((func7 as u32) << 25)
    }

    fn get_instr_i(opcode: u8, func3: u8, rd: u8, rs1: u8, imm: u32) -> u32 {
        (opcode as u32)
            | ((rd as u32) << 7)
            | ((func3 as u32) << 12)
            | ((rs1 as u32) << 15)
            | (imm << 20)
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

        fn test_instr_r(&mut self, instr_kind: Riscv32Instr, opcode: u8, func3: u8, func7: u8) {
            let rd = self.rng.random_range(0..=0b11111) as u8;
            let rs1 = self.rng.random_range(0..=0b11111) as u8;
            let rs2 = self.rng.random_range(0..=0b11111) as u8;

            let instr = get_instr_r(opcode, func3, func7, rd, rs1, rs2);
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

        fn test_instr_i(&mut self, instr_kind: Riscv32Instr, opcode: u8, func3: u8) {
            let rd = self.rng.random_range(0..=0b11111) as u8;
            let rs1 = self.rng.random_range(0..=0b11111) as u8;
            let imm = self.rng.random_range(0..=0xFFF) as u32;

            let instr = get_instr_i(opcode, func3, rd, rs1, imm);
            self.check(
                instr,
                instr_kind,
                RVInstrInfo::I {
                    rs1: rs1,
                    rd: rd,
                    imm: imm,
                },
            );
        }
    }

    #[test]
    fn test_decoder() {
        let mut checker = Checker::new();

        for _ in 1..=100 {
            checker.test_instr_r(Riscv32Instr::ADD, 0b0110011, 0b000, 0b0000000);
            checker.test_instr_r(Riscv32Instr::SUB, 0b0110011, 0b000, 0b0100000);
            checker.test_instr_i(Riscv32Instr::ADDI, 0b0010011, 0b000);
        }
    }
}
