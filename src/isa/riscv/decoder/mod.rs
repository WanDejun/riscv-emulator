use std::fmt::Display;

use crate::{
    config::arch_config::WordType,
    debug_unreachable,
    isa::{
        InstrLen,
        cache::Cacheable,
        riscv::{
            RawInstr,
            decoder::compress_decoder::CompressedDecoder,
            instruction::{InstrFormat, RVInstrInfo, instr_table::*},
        },
        utils::ISABuilder,
    },
    utils::sign_extend,
};

mod compress_decoder;
mod funct_decoder;
mod mask_decoder;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeInstr(pub RiscvInstr, pub RVInstrInfo);

impl Cacheable for DecodeInstr {
    const ADDR_SHIFT_BITS: usize = 1;
}

impl Display for DecodeInstr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}, {:?}", self.0, self.1)
    }
}

pub struct Decoder {
    funct3_decoder: funct_decoder::Decoder,
    mask_decoder: mask_decoder::MaskDecoder,
    compress_decoder: compress_decoder::CompressedDecoder,
}

impl Decoder {
    pub fn new() -> Self {
        let isa_builder = ISABuilder::new()
            .add(TABLE_RV32I)
            .add(TABLE_RV64I)
            .add(TABLE_RVZIFENCEI)
            .add(TABLE_RV32M)
            .add(TABLE_RV64M)
            .add(TABLE_RVZICSR)
            .add(TABLE_RVSYSTEM)
            .add(TABLE_RV32F)
            .add(TABLE_RV64F)
            .add(TABLE_RV32D)
            .add(TABLE_RV64D)
            .add(TABLE_RVS)
            .add(TABLE_RV32A)
            .add(TABLE_RV64A)
            .add(TABLE_RVC)
            // .add(TABLE_RV32C)  // 32 and 64 bit version is conflicting in C extension.
            .add(TABLE_RV64C)
            // .add(TABLE_RV32C_F)
            .add(TABLE_RVC_D)
            // Canonical illegal instruction (0x0000); see data/instr_dict_illegal.json.
            .add(TABLE_RVILLEGAL);

        #[cfg(feature = "custom-instr")]
        let isa_builder = isa_builder.add(TABLE_RVCUSTOM0).add(TABLE_RVCUSTOM1);

        let isa = isa_builder.build();

        Self::from_isa(isa)
    }
}

fn is_compressed(desc: &RVInstrDesc) -> bool {
    desc.key & 0b11 != 0b11
}

impl Decoder {
    pub fn from_isa(instrs: Vec<RVInstrDesc>) -> Self {
        Self {
            compress_decoder: CompressedDecoder::from_isa(
                instrs.iter().filter(|d| is_compressed(d)).cloned(),
            ),
            mask_decoder: mask_decoder::MaskDecoder::from_isa(
                instrs
                    .iter()
                    .filter(|d| !is_compressed(d) && d.use_mask)
                    .cloned(),
            ),
            funct3_decoder: funct_decoder::Decoder::from_isa(
                instrs
                    .iter()
                    .filter(|d| !is_compressed(d) && !d.use_mask)
                    .cloned(),
            ),
        }
    }

    pub fn decode(&self, instr: RawInstr) -> Option<DecodeInstr> {
        if instr.len() == 2 {
            self.compress_decoder.decode(instr)
        } else {
            None.or_else(|| self.mask_decoder.decode(instr))
                .or_else(|| self.funct3_decoder.decode(instr))
        }
    }
}

/// This function doesn't handle compressed instruction.
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
        InstrFormat::A => {
            let aq = ((raw_instr >> 26) & 1) != 0;
            let rl = ((raw_instr >> 25) & 1) != 0;
            RVInstrInfo::A {
                rs1,
                rs2,
                rd,
                aq,
                rl,
            }
        }
        InstrFormat::I => {
            let mut imm = ((raw_instr >> 20) & 0xFFF) as WordType;

            match instr {
                RiscvInstr::SRLI | RiscvInstr::SRAI => {
                    imm &= 0x3F;
                }
                RiscvInstr::SRLIW | RiscvInstr::SRAIW => {
                    imm &= 0x1F;
                }
                _ => {}
            }

            RVInstrInfo::I {
                rd: rd,
                rs1: rs1,
                imm: sign_extend(imm, 12),
            }
        }
        InstrFormat::S => {
            let imm = (((raw_instr >> 25) & 0xFF) << 5) | ((raw_instr >> 7) & 0b11111);
            RVInstrInfo::S {
                rs1: rs1,
                rs2: rs2,
                imm: sign_extend(imm as WordType, 12),
            }
        }
        InstrFormat::U => {
            let imm = (raw_instr >> 12) << 12;
            RVInstrInfo::U {
                rd: rd,
                imm: sign_extend(imm as WordType, 32),
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
                imm: sign_extend(imm as WordType, 13),
            }
        }
        InstrFormat::J => {
            let imm = (((raw_instr >> 31) & 1) << 20)
                | (((raw_instr >> 12) & 0xFF) << 12)
                | (((raw_instr >> 20) & 1) << 11)
                | (((raw_instr >> 21) & 0x3FF) << 1);
            RVInstrInfo::J {
                rd: rd,
                imm: sign_extend(imm as WordType, 21),
            }
        }
        InstrFormat::None => RVInstrInfo::None,

        InstrFormat::CA
        | InstrFormat::CB
        | InstrFormat::CI
        | InstrFormat::CIW
        | InstrFormat::CJ
        | InstrFormat::CL
        | InstrFormat::CR
        | InstrFormat::CS
        | InstrFormat::CSS => debug_unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        isa::riscv::{
            csr_reg::csr_index,
            instruction::{RVInstrInfo, instr_table::RiscvInstr},
        },
        utils::negative_of,
    };

    use super::*;

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
    }

    impl Checker {
        fn new() -> Self {
            Checker {
                decoder: Decoder::new(),
            }
        }

        fn check(&mut self, instr: u32, expected: RiscvInstr, expected_info: RVInstrInfo) {
            let result = self.decoder.decode(instr.into()).unwrap();
            assert_eq!(result, DecodeInstr(expected, expected_info));
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
                imm: negative_of(5),
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
                imm: negative_of(112),
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

        checker.check(
            0x0000000f,
            RiscvInstr::FENCE,
            RVInstrInfo::I {
                rs1: 0,
                rd: 0,
                imm: 0,
            },
        );

        checker.check(
            0x0000100f,
            RiscvInstr::FENCE_I,
            RVInstrInfo::I {
                rs1: 0,
                rd: 0,
                imm: 0,
            },
        );
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

    #[test]
    fn test_decoder_rva() {
        let mut checker = Checker::new();

        checker.check(
            0xef537af, // amoswap.d a5, a5, (a0)
            RiscvInstr::AMOSWAP_D,
            RVInstrInfo::A {
                rd: 15,
                rs1: 10,
                rs2: 15,
                aq: true,
                rl: true,
            },
        );
    }

    // All raw encodings below were produced by `riscv64-unknown-elf-as -march=rv64gc`
    // and dumped with objdump, so they are real assembler output rather than guesses.
    // ABI->x mapping used in the comments: t0=x5 t1=x6 s0=x8 s1=x9 a0=x10 a2=x12 a5=x15 sp=x2.
    #[test]
    fn test_decoder_rvc_arith() {
        let mut checker = Checker::new();

        // c.addi s0,5
        checker.check(
            0x0415,
            RiscvInstr::C_ADDI,
            RVInstrInfo::CI { rd_rs1: 8, imm: 5 },
        );
        // c.addi a5,-1
        checker.check(
            0x17fd,
            RiscvInstr::C_ADDI,
            RVInstrInfo::CI {
                rd_rs1: 15,
                imm: negative_of(1),
            },
        );
        // c.li a0,-3
        checker.check(
            0x5575,
            RiscvInstr::C_LI,
            RVInstrInfo::CI {
                rd_rs1: 10,
                imm: negative_of(3),
            },
        );
        // c.lui a2,0x10  (decoder leaves the value already shifted left by 12)
        checker.check(
            0x6641,
            RiscvInstr::C_LUI,
            RVInstrInfo::CI {
                rd_rs1: 12,
                imm: 0x10 << 12,
            },
        );
        // c.addi16sp sp,-16
        checker.check(
            0x717d,
            RiscvInstr::C_ADDI16SP,
            RVInstrInfo::CI {
                rd_rs1: 2,
                imm: negative_of(16),
            },
        );
        // c.addi4spn s0,sp,16
        checker.check(
            0x0800,
            RiscvInstr::C_ADDI4SPN,
            RVInstrInfo::CIW { rd: 8, imm: 16 },
        );
        // c.slli t0,0x3
        checker.check(
            0x028e,
            RiscvInstr::C_SLLI,
            RVInstrInfo::CI { rd_rs1: 5, imm: 3 },
        );
        // c.srli s0,0x2
        checker.check(
            0x8009,
            RiscvInstr::C_SRLI,
            RVInstrInfo::CB { rd_rs1: 8, imm: 2 },
        );
        // c.srai s1,0x4
        checker.check(
            0x8491,
            RiscvInstr::C_SRAI,
            RVInstrInfo::CB { rd_rs1: 9, imm: 4 },
        );
        // c.andi s0,-1
        checker.check(
            0x987d,
            RiscvInstr::C_ANDI,
            RVInstrInfo::CB {
                rd_rs1: 8,
                imm: negative_of(1),
            },
        );
        // c.mv t0,t1
        checker.check(
            0x829a,
            RiscvInstr::C_MV,
            RVInstrInfo::CR { rd_rs1: 5, rs2: 6 },
        );
        // c.add t0,t1
        checker.check(
            0x929a,
            RiscvInstr::C_ADD,
            RVInstrInfo::CR { rd_rs1: 5, rs2: 6 },
        );
        // c.and / c.or / c.xor / c.sub / c.addw / c.subw s0,s1
        checker.check(
            0x8c65,
            RiscvInstr::C_AND,
            RVInstrInfo::CA { rd_rs1: 8, rs2: 9 },
        );
        checker.check(
            0x8c45,
            RiscvInstr::C_OR,
            RVInstrInfo::CA { rd_rs1: 8, rs2: 9 },
        );
        checker.check(
            0x8c25,
            RiscvInstr::C_XOR,
            RVInstrInfo::CA { rd_rs1: 8, rs2: 9 },
        );
        checker.check(
            0x8c05,
            RiscvInstr::C_SUB,
            RVInstrInfo::CA { rd_rs1: 8, rs2: 9 },
        );
        checker.check(
            0x9c25,
            RiscvInstr::C_ADDW,
            RVInstrInfo::CA { rd_rs1: 8, rs2: 9 },
        );
        checker.check(
            0x9c05,
            RiscvInstr::C_SUBW,
            RVInstrInfo::CA { rd_rs1: 8, rs2: 9 },
        );
    }

    #[test]
    fn test_decoder_rvc_load_store() {
        let mut checker = Checker::new();

        // c.lw s0,4(s1)
        checker.check(
            0x40c0,
            RiscvInstr::C_LW,
            RVInstrInfo::CL {
                rd: 8,
                rs1: 9,
                imm: 4,
            },
        );
        // c.ld s0,8(s1)
        checker.check(
            0x6480,
            RiscvInstr::C_LD,
            RVInstrInfo::CL {
                rd: 8,
                rs1: 9,
                imm: 8,
            },
        );
        // c.sw s0,4(s1)
        checker.check(
            0xc0c0,
            RiscvInstr::C_SW,
            RVInstrInfo::CS {
                rs1: 9,
                rs2: 8,
                imm: 4,
            },
        );
        // c.sd s0,8(s1)
        checker.check(
            0xe480,
            RiscvInstr::C_SD,
            RVInstrInfo::CS {
                rs1: 9,
                rs2: 8,
                imm: 8,
            },
        );
        // c.lwsp a0,4(sp)
        checker.check(
            0x4512,
            RiscvInstr::C_LWSP,
            RVInstrInfo::CI { rd_rs1: 10, imm: 4 },
        );
        // c.ldsp a0,8(sp)
        checker.check(
            0x6522,
            RiscvInstr::C_LDSP,
            RVInstrInfo::CI { rd_rs1: 10, imm: 8 },
        );
        // c.swsp a0,4(sp)
        checker.check(
            0xc22a,
            RiscvInstr::C_SWSP,
            RVInstrInfo::CSS { rs2: 10, imm: 4 },
        );
        // c.sdsp a0,8(sp)
        checker.check(
            0xe42a,
            RiscvInstr::C_SDSP,
            RVInstrInfo::CSS { rs2: 10, imm: 8 },
        );

        // c.fld fs0,8(s1)
        checker.check(
            0x2480,
            RiscvInstr::C_FLD,
            RVInstrInfo::CL {
                rd: 8,
                rs1: 9,
                imm: 8,
            },
        );
        // c.fsd fs0,8(s1)
        checker.check(
            0xa480,
            RiscvInstr::C_FSD,
            RVInstrInfo::CS {
                rs1: 9,
                rs2: 8,
                imm: 8,
            },
        );
        // c.fldsp fa0,8(sp)
        checker.check(
            0x2522,
            RiscvInstr::C_FLDSP,
            RVInstrInfo::CI { rd_rs1: 10, imm: 8 },
        );
        // c.fsdsp fa0,8(sp)
        checker.check(
            0xa42a,
            RiscvInstr::C_FSDSP,
            RVInstrInfo::CSS { rs2: 10, imm: 8 },
        );
    }

    #[test]
    fn test_decoder_rvc_control() {
        let mut checker = Checker::new();

        // c.jr t0 / c.jalr t0 (rs2 is always 0)
        checker.check(
            0x8282,
            RiscvInstr::C_JR,
            RVInstrInfo::CR { rd_rs1: 5, rs2: 0 },
        );
        checker.check(
            0x9282,
            RiscvInstr::C_JALR,
            RVInstrInfo::CR { rd_rs1: 5, rs2: 0 },
        );

        // PC-relative offsets, computed as (target - instr_addr) from the objdump layout:
        //   0x4: c.j a_back(0x0)       -> -4
        //   0x6: c.beqz s0, a_back     -> -6
        //   0x8: c.bnez s0, a_fwd(0xc) -> +4
        //   0xa: c.j a_fwd(0xc)        -> +2
        checker.check(
            0xbff5,
            RiscvInstr::C_J,
            RVInstrInfo::CJ {
                target: negative_of(4),
            },
        );
        checker.check(
            0xdc6d,
            RiscvInstr::C_BEQZ,
            RVInstrInfo::CB {
                rd_rs1: 8,
                imm: negative_of(6),
            },
        );
        checker.check(
            0xe011,
            RiscvInstr::C_BNEZ,
            RVInstrInfo::CB { rd_rs1: 8, imm: 4 },
        );
        checker.check(0xa009, RiscvInstr::C_J, RVInstrInfo::CJ { target: 2 });
    }

    // Encodings that match more than one mask; `ensure_order` must let the
    // more-specific instruction win the linear scan.
    #[test]
    fn test_decoder_rvc_priority() {
        let mut checker = Checker::new();

        // 0x0001 also matches C_ADDI; must decode as C_NOP.
        checker.check(
            0x0001,
            RiscvInstr::C_NOP,
            RVInstrInfo::CR { rd_rs1: 0, rs2: 0 },
        );
        // 0x9002 also matches C_JALR; must decode as C_EBREAK.
        checker.check(0x9002, RiscvInstr::C_EBREAK, RVInstrInfo::None);
    }
}
