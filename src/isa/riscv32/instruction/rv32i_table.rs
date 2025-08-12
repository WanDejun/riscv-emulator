use crate::{define_instr_enum, define_riscv_isa, isa::riscv32::instruction::InstrFormat};

include!(concat!(env!("OUT_DIR"), "/rvinstr_gen.rs"));

/*
define_riscv_isa!(
    RiscvInstr,
    RV32I, TABLE_RV32I, {
        // Arithmetic
        ADD {
            opcode: 51,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::R,
        },
        ADDI {
            opcode: 19,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::I,
        },
        SUB {
            opcode: 51,
            funct3: 0,
            funct7: 32,
            format: InstrFormat::R,
        },

        // Shift
        SLL {
            opcode: 51,
            funct3: 1,
            funct7: 0,
            format: InstrFormat::R,
        },
        SRL {
            opcode: 51,
            funct3: 5,
            funct7: 0,
            format: InstrFormat::R,
        },
        SRA {
            opcode: 51,
            funct3: 5,
            funct7: 32,
            format: InstrFormat::R,
        },

        // Cond set
        SLT {
            opcode: 51,
            funct3: 2,
            funct7: 0,
            format: InstrFormat::R,

        },
        SLTI {
            opcode: 19,
            funct3: 2,
            funct7: 0,
            format: InstrFormat::I,

        },
        SLTIU {
            opcode: 19,
            funct3: 3,
            funct7: 0,
            format: InstrFormat::I,

        },
        SLTU {
            opcode: 51,
            funct3: 3,
            funct7: 0,
            format: InstrFormat::R,

        },

        // Bit
        AND {
            opcode: 51,
            funct3: 7,
            funct7: 0,
            format: InstrFormat::R,

        },
        ANDI {
            opcode: 19,
            funct3: 7,
            funct7: 0,
            format: InstrFormat::I,

        },
        OR {
            opcode: 51,
            funct3: 6,
            funct7: 0,
            format: InstrFormat::R,

        },
        ORI {
            opcode: 19,
            funct3: 6,
            funct7: 0,
            format: InstrFormat::I,

        },
        XOR {
            opcode: 51,
            funct3: 4,
            funct7: 0,
            format: InstrFormat::R,

        },
        XORI {
            opcode: 19,
            funct3: 4,
            funct7: 0,
            format: InstrFormat::I,

        },

        // Branch
        BEQ {
            opcode: 99,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::B,

        },
        BNE {
            opcode: 99,
            funct3: 1,
            funct7: 0,
            format: InstrFormat::B,

        },
        BGE {
            opcode: 99,
            funct3: 5,
            funct7: 0,
            format: InstrFormat::B,

        },
        BGEU {
            opcode: 99,
            funct3: 7,
            funct7: 0,
            format: InstrFormat::B,

        },
        BLT {
            opcode: 99,
            funct3: 4,
            funct7: 0,
            format: InstrFormat::B,

        },
        BLTU {
            opcode: 99,
            funct3: 6,
            funct7: 0,
            format: InstrFormat::B,

        },

        // Load
        LB {
            opcode: 3,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::I,

        },
        LBU {
            opcode: 3,
            funct3: 4,
            funct7: 0,
            format: InstrFormat::I,

        },
        LH {
            opcode: 3,
            funct3: 1,
            funct7: 0,
            format: InstrFormat::I,

        },
        LHU {
            opcode: 3,
            funct3: 5,
            funct7: 0,
            format: InstrFormat::I,

        },
        LW {
            opcode: 3,
            funct3: 2,
            funct7: 0,
            format: InstrFormat::I,

        },

        // Store
        SB {
            opcode: 35,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::S,

        },
        SH {
            opcode: 35,
            funct3: 1,
            funct7: 0,
            format: InstrFormat::S,

        },
        SW {
            opcode: 35,
            funct3: 2,
            funct7: 0,
            format: InstrFormat::S,

        },

        // Jump and link
        JAL {
            opcode: 111,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::J,
        },
        JALR {
            opcode: 103,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::I,
        },

        // U-Type
        AUIPC {
            opcode: 23,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::U,
        },

        LUI {
            opcode: 55,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::U,
        },
        EBREAK {
            opcode: 115,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::I,
        },
        ECALL {
            opcode: 115,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::I,
        },
        FENCE {
            opcode: 15,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::I,
        },
    },
    RV32M, TABLE_RV32M, {
        DIV {
            opcode: 51,
            funct3: 4,
            funct7: 1,
            format: InstrFormat::R,
        },
        DIVU {
            opcode: 51,
            funct3: 5,
            funct7: 1,
            format: InstrFormat::R,
        },
        MUL {
            opcode: 51,
            funct3: 0,
            funct7: 1,
            format: InstrFormat::R,
        },
        MULH {
            opcode: 51,
            funct3: 1,
            funct7: 1,
            format: InstrFormat::R,
        },
        MULHSU {
            opcode: 51,
            funct3: 2,
            funct7: 1,
            format: InstrFormat::R,
        },
        MULHU {
            opcode: 51,
            funct3: 3,
            funct7: 1,
            format: InstrFormat::R,
        },
        REM {
            opcode: 51,
            funct3: 6,
            funct7: 1,
            format: InstrFormat::R,
        },
        REMU {
            opcode: 51,
            funct3: 7,
            funct7: 1,
            format: InstrFormat::R,
        },
    },
    RV64I, TABLE_RV64I, {
        ADDIW {
            opcode: 27,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::I,
        },
        ADDW {
            opcode: 59,
            funct3: 0,
            funct7: 0,
            format: InstrFormat::R,
        },
        LD {
            opcode: 3,
            funct3: 3,
            funct7: 0,
            format: InstrFormat::I,
        },
        LWU {
            opcode: 3,
            funct3: 6,
            funct7: 0,
            format: InstrFormat::I,
        },
        SD {
            opcode: 35,
            funct3: 3,
            funct7: 0,
            format: InstrFormat::S,

        },
        SLLI {
            opcode: 19,
            funct3: 1,
            funct7: 0,
            format: InstrFormat::I,
        },
        SLLIW {
            opcode: 27,
            funct3: 1,
            funct7: 0,
            format: InstrFormat::I,
        },
        SLLW {
            opcode: 59,
            funct3: 1,
            funct7: 0,
            format: InstrFormat::R,
        },
        SRAI {
            opcode: 19,
            funct3: 5,
            funct7: 0,
            format: InstrFormat::I,
        },
        SRAIW {
            opcode: 27,
            funct3: 5,
            funct7: 32,
            format: InstrFormat::I,
        },
        SRAW {
            opcode: 59,
            funct3: 5,
            funct7: 32,
            format: InstrFormat::R,
        },
        SRLI {
            opcode: 19,
            funct3: 5,
            funct7: 0,
            format: InstrFormat::I,
        },
        SRLIW {
            opcode: 27,
            funct3: 5,
            funct7: 0,
            format: InstrFormat::I,
        },
        SRLW {
            opcode: 59,
            funct3: 5,
            funct7: 0,
            format: InstrFormat::R,
        },
        SUBW {
            opcode: 59,
            funct3: 0,
            funct7: 32,
            format: InstrFormat::R,
        },
    },
);
*/
