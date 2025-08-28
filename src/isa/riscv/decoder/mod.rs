use std::fmt::Display;

use crate::{
    config::arch_config::WordType,
    isa::{
        DecoderTrait,
        riscv::{
            RiscvTypes,
            instruction::{InstrFormat, RVInstrInfo, rv32i_table::*},
        },
        utils::ISABuilder,
    },
};

mod funct_decoder;
mod mask_decoder;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeInstr(pub RiscvInstr, pub RVInstrInfo);

impl Display for DecodeInstr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}, {:?}", self.0, self.1)
    }
}

pub struct Decoder {
    funct3_decoder: funct_decoder::Decoder,
    mask_decoder: mask_decoder::MaskDecoder,
}

impl Decoder {
    pub fn new() -> Self {
        let isa = ISABuilder::new()
            .add(TABLE_RV32I)
            .add(TABLE_RV64I)
            .add(TABLE_RV32M)
            .add(TABLE_RV64M)
            .add(TABLE_RVZICSR)
            .add(TABLE_RVSYSTEM)
            .add(TABLE_RV32F)
            .add(TABLE_RV64F)
            .build();
        Self {
            funct3_decoder: funct_decoder::Decoder::from_isa(&isa),
            mask_decoder: mask_decoder::MaskDecoder::from_isa(&isa),
        }
    }
}

impl DecoderTrait<RiscvTypes> for Decoder {
    fn from_isa(instrs: &[RV32Desc]) -> Self {
        Self {
            funct3_decoder: funct_decoder::Decoder::from_isa(instrs),
            mask_decoder: mask_decoder::MaskDecoder::from_isa(instrs),
        }
    }

    fn decode(&self, instr: u32) -> Option<DecodeInstr> {
        None.or_else(|| self.mask_decoder.decode(instr))
            .or_else(|| self.funct3_decoder.decode(instr))
    }
}

fn decode_info(raw_instr: u32, instr: RiscvInstr, fmt: InstrFormat) -> RVInstrInfo {
    let rd = ((raw_instr >> 7) & 0b11111) as u8;
    let rs1 = ((raw_instr >> 15) & 0b11111) as u8;
    let rs2 = ((raw_instr >> 20) & 0b11111) as u8;
    let f3 = ((raw_instr >> 12) & 0b111) as u8;

    match fmt {
        InstrFormat::R => RVInstrInfo::R { rd, rs1, rs2 },
        InstrFormat::R_rm => RVInstrInfo::R_rm {
            rs1,
            rs2,
            rd,
            rm: f3,
        },
        InstrFormat::R4_rm => RVInstrInfo::R4_rm {
            rd,
            rs1,
            rs2,
            rs3: ((raw_instr >> 27) & 0b11111) as u8,
            rm: f3,
        },
        InstrFormat::I => {
            let mut imm = ((raw_instr >> 20) & 0xFFF) as WordType;

            match instr {
                RiscvInstr::SRAI | RiscvInstr::SRAIW => {
                    imm &= 0x1F;
                }
                _ => {}
            }

            RVInstrInfo::I {
                rd: rd,
                rs1: rs1,
                imm: imm,
            }
        }
        InstrFormat::S => {
            let imm = (((raw_instr >> 25) & 0xFF) << 5) | ((raw_instr >> 7) & 0b11111);
            RVInstrInfo::S {
                rs1: rs1,
                rs2: rs2,
                imm: imm as WordType,
            }
        }
        InstrFormat::U => {
            let imm = (raw_instr >> 12) << 12;
            RVInstrInfo::U {
                rd: rd,
                imm: imm as WordType,
            }
        }
        InstrFormat::B => {
            let imm = (((raw_instr >> 31) & 1) << 12)
                | (((raw_instr >> 7) & 1) << 11)
                | (((raw_instr >> 25) & 0b111111) << 5)
                | (((raw_instr >> 8) & 0b1111) << 1);
            RVInstrInfo::B {
                rs1: rs1,
                rs2: rs2,
                imm: imm as WordType,
            }
        }
        InstrFormat::J => {
            let imm = (((raw_instr >> 31) & 1) << 20)
                | (((raw_instr >> 12) & 0xFF) << 12)
                | (((raw_instr >> 20) & 1) << 11)
                | (((raw_instr >> 21) & 0x3FF) << 1);
            RVInstrInfo::J {
                rd: rd,
                imm: imm as WordType,
            }
        }
        InstrFormat::None => RVInstrInfo::None,
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        config::arch_config::WordType,
        isa::riscv::{
            csr_reg::csr_index,
            instruction::{RVInstrInfo, rv32i_table::RiscvInstr},
        },
        utils::{TruncateTo, negative_of},
    };

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

        fn check(&mut self, instr: u32, expected: RiscvInstr, expected_info: RVInstrInfo) {
            let result = self.decoder.decode(instr).unwrap();
            assert_eq!(result, DecodeInstr(expected, expected_info));
        }

        fn test_instr_r(&mut self, instr_kind: RiscvInstr, opcode: u8, funct3: u8, funct7: u8) {
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

        fn test_instr_i(&mut self, instr_kind: RiscvInstr, opcode: u8, funct3: u8) {
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

        fn test_instr_s(&mut self, instr_kind: RiscvInstr, opcode: u8, funct3: u8) {
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

        fn test_instr_b(&mut self, instr_kind: RiscvInstr, opcode: u8, funct3: u8) {
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

        fn test_instr_u(&mut self, instr_kind: RiscvInstr, opcode: u8) {
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

        fn test_instr_j(&mut self, instr_kind: RiscvInstr, opcode: u8) {
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
            checker.test_instr_r(RiscvInstr::ADD, 0b0110011, 0b000, 0b0000000);
            checker.test_instr_r(RiscvInstr::SUB, 0b0110011, 0b000, 0b0100000);

            checker.test_instr_i(RiscvInstr::ADDI, 0b0010011, 0b000);
            checker.test_instr_i(RiscvInstr::ORI, 0b0010011, 0b110);

            checker.test_instr_s(RiscvInstr::SB, 0b0100011, 0b000);
            checker.test_instr_s(RiscvInstr::SW, 0b0100011, 0b010);

            checker.test_instr_b(RiscvInstr::BNE, 0b1100011, 0b001);

            checker.test_instr_u(RiscvInstr::LUI, 0b0110111);

            checker.test_instr_j(RiscvInstr::JAL, 0b1101111);
        }
    }

    #[test]
    fn test_decoder_rv32i() {
        let mut checker = Checker::new();

        checker.check(
            0x123450b7,
            RiscvInstr::LUI,
            RVInstrInfo::U {
                rd: 1,
                imm: 0x12345000,
            },
        );

        checker.check(
            0x12233097,
            RiscvInstr::AUIPC,
            RVInstrInfo::U {
                rd: 1,
                imm: 0x12233000,
            },
        );

        checker.check(
            0xffb18113,
            RiscvInstr::ADDI,
            RVInstrInfo::I {
                rs1: 3,
                rd: 2,
                imm: negative_of(5).truncate_to(12),
            },
        );

        checker.check(
            0x00210083,
            RiscvInstr::LB,
            RVInstrInfo::I {
                rs1: 2,
                rd: 1,
                imm: 2,
            },
        );

        checker.check(
            0xf8c318e3,
            RiscvInstr::BNE,
            RVInstrInfo::B {
                rs1: 6,
                rs2: 12,
                imm: negative_of(112).truncate_to(13),
            },
        );

        checker.check(
            0x0207d793, // srli	a5,a5,0x20
            RiscvInstr::SRLI,
            RVInstrInfo::I {
                rs1: 15,
                rd: 15,
                imm: 0x20,
            },
        );

        checker.check(0x100073, RiscvInstr::EBREAK, RVInstrInfo::None);
        checker.check(0x000073, RiscvInstr::ECALL, RVInstrInfo::None);
    }

    #[test]
    fn test_decoder_privilege() {
        let mut checker = Checker::new();

        checker.check(0x30200073, RiscvInstr::MRET, RVInstrInfo::None);
    }

    #[test]
    fn test_decoder_rv64i() {
        let mut checker = Checker::new();

        checker.check(
            0x4027d79b, //sraiw	a5,a5,0x2
            RiscvInstr::SRAIW,
            RVInstrInfo::I {
                rs1: 15,
                rd: 15,
                imm: 2,
            },
        );
    }

    #[test]
    fn test_decoder_rv64f() {
        let mut checker = Checker::new();

        checker.check(
            0x00b576d3, // fadd.s fa3,fa0,fa1
            RiscvInstr::FADD_S,
            RVInstrInfo::R_rm {
                rd: 13,
                rs1: 10,
                rs2: 11,
                rm: 7,
            },
        );
    }

    #[test]
    fn test_deocder_csr() {
        let mut checker = Checker::new();

        checker.check(
            0x001015f3, // fsflags a1,zero => csrrw a1, fflags, zero
            RiscvInstr::CSRRW,
            RVInstrInfo::I {
                rs1: 0,
                rd: 11,
                imm: csr_index::fflags,
            },
        );

        checker.check(
            0xe0068553, // fmv.x.w a0,fa3
            RiscvInstr::FMV_X_W,
            RVInstrInfo::R {
                rs1: 13,
                rs2: 0,
                rd: 10,
            },
        );
    }
}
