use crate::{
    config::arch_config::WordType,
    define_instr_enum, define_riscv_isa,
    isa::riscv32::{
        executor::RV32CPU,
        instruction::{Exception, InstrFormat, RVInstrInfo, exec_function::*},
    },
    utils::sign_extend,
};

define_riscv_isa!(
    Riscv32Instr,
    RV32I, TABLE_RV32I, {
        // Arithmetic
        ADD {
            opcode: 51,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::R,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecAdd>(inst_info, cpu)},
        },
        ADDI {
            opcode: 19,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecAdd>(inst_info, cpu)},
        },
        SUB {
            opcode: 51,
            funct3: 0,
            funct7: 32,
            format: InstrFormat::R,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecSub>(inst_info, cpu)},
        },

        // Shift
        SLL {
            opcode: 51,
            funct3: 1,
            funct7: 0,
            format: InstrFormat::R,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecSLL>(inst_info, cpu)},
        },
        SRL {
            opcode: 51,
            funct3: 5,
            funct7: 0,
            format: InstrFormat::R,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecSRL>(inst_info, cpu)},
        },
        SRA {
            opcode: 51,
            funct3: 5,
            funct7: 32,
            format: InstrFormat::R,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecSRA>(inst_info, cpu)},
        },

        // Cond set
        SLT {
            opcode: 51,
            funct3: 2,
            funct7: 0,
            format: InstrFormat::R,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecSignedLess>(inst_info, cpu)},
        },
        SLTI {
            opcode: 19,
            funct3: 2,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecSignedLess>(inst_info, cpu)},
        },
        SLTIU {
            opcode: 19,
            funct3: 3,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecUnsignedLess>(inst_info, cpu)},
        },
        SLTU {
            opcode: 51,
            funct3: 3,
            funct7: 0,
            format: InstrFormat::R,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecUnsignedLess>(inst_info, cpu)},
        },

        // Bit
        AND {
            opcode: 51,
            funct3: 7,
            funct7: 0,
            format: InstrFormat::R,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecAnd>(inst_info, cpu)},
        },
        ANDI {
            opcode: 19,
            funct3: 7,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecAnd>(inst_info, cpu)},
        },
        OR {
            opcode: 51,
            funct3: 6,
            funct7: 0,
            format: InstrFormat::R,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecOr>(inst_info, cpu)},
        },
        ORI {
            opcode: 19,
            funct3: 6,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecOr>(inst_info, cpu)},
        },
        XOR {
            opcode: 51,
            funct3: 4,
            funct7: 0,
            format: InstrFormat::R,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecXor>(inst_info, cpu)},
        },
        XORI {
            opcode: 19,
            funct3: 4,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecXor>(inst_info, cpu)},
        },

        // Branch
        BEQ {
            opcode: 99,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::B,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_branch::<ExecEqual>(inst_info, cpu)},
        },
        BNE {
            opcode: 99,
            funct3: 1,
            funct7: 0,
            format: InstrFormat::B,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_branch::<ExecNotEqual>(inst_info, cpu)},
        },
        BGE {
            opcode: 99,
            funct3: 5,
            funct7: 0,
            format: InstrFormat::B,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_branch::<ExecSignedGreatEqual>(inst_info, cpu)},
        },
        BGEU {
            opcode: 99,
            funct3: 7,
            funct7: 0,
            format: InstrFormat::B,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_branch::<ExecUnsignedGreatEqual>(inst_info, cpu)},
        },
        BLT {
            opcode: 99,
            funct3: 4,
            funct7: 0,
            format: InstrFormat::B,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_branch::<ExecSignedLess>(inst_info, cpu)},
        },
        BLTU {
            opcode: 99,
            funct3: 6,
            funct7: 0,
            format: InstrFormat::B,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_branch::<ExecUnsignedLess>(inst_info, cpu)},
        },

        // Load
        LB {
            opcode: 3,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_load::<u8, true>(inst_info, cpu)},
        },
        LBU {
            opcode: 3,
            funct3: 4,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_load::<u8, false>(inst_info, cpu)},
        },
        LH {
            opcode: 3,
            funct3: 1,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_load::<u16, true>(inst_info, cpu)},
        },
        LHU {
            opcode: 3,
            funct3: 5,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_load::<u16, false>(inst_info, cpu)},
        },
        LW {
            opcode: 3,
            funct3: 2,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_load::<u32, false>(inst_info, cpu)},
        },

        // Store
        SB {
            opcode: 35,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::S,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_store::<u8>(inst_info, cpu)},
        },
        SH {
            opcode: 35,
            funct3: 1,
            funct7: 0,
            format: InstrFormat::S,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_store::<u16>(inst_info, cpu)},
        },
        SW {
            opcode: 35,
            funct3: 2,
            funct7: 0,
            format: InstrFormat::S,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_store::<u32>(inst_info, cpu)},
        },

        // Jump and link
        JAL {
            opcode: 111,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::J,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {
                if let RVInstrInfo::J { rd, imm } = inst_info {
                    cpu.reg_file.write(rd, cpu.pc.wrapping_add(4));
                    cpu.pc = cpu.pc.wrapping_add(sign_extend(imm, 21));
                } else {
                    std::unreachable!();
                }
                Ok(())
            },
        },
        JALR {
            opcode: 103,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {
                if let RVInstrInfo::I { rs1, rd, imm } = inst_info {
                    let t = cpu.pc + 4;
                    let val = cpu.reg_file.read(rs1, 0).0;
                    cpu.pc = (val.wrapping_add(sign_extend(imm, 12)) & !1) as WordType;
                    cpu.reg_file.write(rd, t);
                } else {
                    std::unreachable!();
                }

                Ok(())
            },
        },

        // U-Type
        AUIPC {
            opcode: 23,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::U,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {
                if let RVInstrInfo::U { rd, imm } = inst_info {
                    cpu.reg_file
                        .write(rd, cpu.pc.wrapping_add(sign_extend(imm, 32)));
                    cpu.pc = cpu.pc.wrapping_add(4);
                    Ok(())
                } else {
                    std::unreachable!();
                }
            },
        },

        LUI {
            opcode: 55,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::U,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {
                if let RVInstrInfo::U { rd, imm } = inst_info {
                    cpu.reg_file.write(rd, sign_extend(imm, 32));
                    cpu.pc = cpu.pc.wrapping_add(4);
                    Ok(())
                } else {
                    std::unreachable!();
                }
            },
        },
        EBREAK {
            opcode: 115,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_todo::<()>(inst_info, cpu)},
        },
        ECALL {
            opcode: 115,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_todo::<()>(inst_info, cpu)},
        },
        FENCE {
            opcode: 15,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_todo::<()>(inst_info, cpu)},
        },
    },
    RV32M, TABLE_RV32M, {
        DIV {
            opcode: 51,
            funct3: 4,
            funct7: 1,
            format: InstrFormat::R,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecDivSigned>(inst_info, cpu)},
        },
        DIVU {
            opcode: 51,
            funct3: 5,
            funct7: 1,
            format: InstrFormat::R,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecDivUnsigned>(inst_info, cpu)},
        },
        MUL {
            opcode: 51,
            funct3: 0,
            funct7: 1,
            format: InstrFormat::R,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecMulLow>(inst_info, cpu)},
        },
        MULH {
            opcode: 51,
            funct3: 1,
            funct7: 1,
            format: InstrFormat::R,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecMulHighSighed>(inst_info, cpu)},
        },
        MULHSU {
            opcode: 51,
            funct3: 2,
            funct7: 1,
            format: InstrFormat::R,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecMulHighSignedUnsigned>(inst_info, cpu)},
        },
        MULHU {
            opcode: 51,
            funct3: 3,
            funct7: 1,
            format: InstrFormat::R,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecMulHighUnsigned>(inst_info, cpu)},
        },
        REM {
            opcode: 51,
            funct3: 6,
            funct7: 1,
            format: InstrFormat::R,
            callback: |inst_info: RVInstrInfo, cpu: &mut RV32CPU | {exec_arith::<ExecRemSigned>(inst_info, cpu)},
        },
        REMU {
            opcode: 51,
            funct3: 7,
            funct7: 1,
            format: InstrFormat::R,
            callback: |inst_info, cpu | {exec_arith::<ExecRemUnsigned>(inst_info, cpu)},
        },
    },
    RV64I, TABLE_RV64I, {
        ADDIW {
            opcode: 27,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info, cpu| { exec_arith::<ExecAdd>(inst_info, cpu) },
        },
        ADDW {
            opcode: 59,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::R,
            callback: |inst_info, cpu| { exec_arith::<ExecAdd>(inst_info, cpu) },
        },
        LD {
            opcode: 3,
            funct3: 3,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info, cpu| { exec_load::<u64, false>(inst_info, cpu) },
        },
        LWU {
            opcode: 3,
            funct3: 6,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info, cpu| { exec_load::<u32, false>(inst_info, cpu) },
        },
        SD {
            opcode: 35,
            funct3: 3,
            funct7: 0,
            format: InstrFormat::S,
            callback: |inst_info, cpu| { exec_store::<u64>(inst_info, cpu) },

        },
        SLLI {
            opcode: 19,
            funct3: 1,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info, cpu| { exec_arith::<ExecSLL>(inst_info, cpu) },
        },
        SLLIW {
            opcode: 27,
            funct3: 1,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info, cpu| { exec_arith::<ExecSLLW>(inst_info, cpu) },
        },
        SLLW {
            opcode: 59,
            funct3: 1,
            funct7: 0,
            format: InstrFormat::R,
            callback: |inst_info, cpu| { exec_arith::<ExecSLLW>(inst_info, cpu) },
        },
        SRAI {
            opcode: 19,
            funct3: 5,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info, cpu| { exec_arith::<ExecSRA>(inst_info, cpu) },
        },
        SRAIW {
            opcode: 27,
            funct3: 5,
            funct7: 32,
            format: InstrFormat::I,
            callback: |inst_info, cpu| { exec_arith::<ExecSRA>(inst_info, cpu) },
        },
        SRAW {
            opcode: 59,
            funct3: 5,
            funct7: 32,
            format: InstrFormat::R,
            callback: |inst_info, cpu| { exec_arith::<ExecSRA>(inst_info, cpu) },
        },
        SRLI {
            opcode: 19,
            funct3: 5,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info, cpu| { exec_arith::<ExecSRL>(inst_info, cpu) },
        },
        SRLIW {
            opcode: 27,
            funct3: 5,
            funct7: 0,
            format: InstrFormat::I,
            callback: |inst_info, cpu| { exec_arith::<ExecSRLW>(inst_info, cpu) },
        },
        SRLW {
            opcode: 59,
            funct3: 5,
            funct7: 0,
            format: InstrFormat::R,
            callback: |inst_info, cpu| { exec_arith::<ExecSRLW>(inst_info, cpu) },
        },
        SUBW {
            opcode: 59,
            funct3: 0,
            funct7: 32,
            format: InstrFormat::R,
            callback: |inst_info, cpu| { exec_arith::<ExecSub>(inst_info, cpu) },
        },
    },
);
