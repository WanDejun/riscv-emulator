use crate::{config::arch_config::WordType, define_instr_enum};

#[derive(Debug, Clone, PartialEq)]
pub enum RVInstrInfo {
    R { rs1: u8, rs2: u8, rd: u8 },
    I { rs1: u8, rd: u8, imm: WordType },
    S { rs1: u8, rs2: u8, imm: WordType },
    B { rs1: u8, rs2: u8, imm: WordType },
    U { rd: u8, imm: WordType },
    J { rd: u8, imm: WordType },
}

#[derive(Debug, Clone, Copy)]
pub enum InstrFormat {
    U,
    J,
    B,
    I,
    S,
    R,
}

// define a single enum for every instruction
// define tables for each instruction set
macro_rules! define_riscv_isa {
    ( $tot_instr_name:ident,
        $( $isa_name:ident, $isa_table_name:ident, {$(
                $name:ident {
                    opcode: $opcode:literal,
                    funct3: $funct3:literal,
                    funct7: $funct7:literal,
                    format: $fmt:expr,
                }),* $(,)?
            }
        ),* $(,)?
    ) => {

        define_instr_enum!($tot_instr_name, $($($name,)*)*);

        #[derive(Debug, Clone)]
        pub struct RV32Desc {
            pub opcode: u8,
            pub funct3: u8,
            pub funct7: u8,
            pub instr: $tot_instr_name,
            pub format: InstrFormat,
        }

        $(
            pub const $isa_table_name: &[RV32Desc] = &[
                $(
                    RV32Desc {
                        opcode: $opcode,
                        funct3: $funct3,
                        funct7: $funct7,
                        instr: $tot_instr_name::$name,
                        format: $fmt,
                    }
                ),*
            ];
        )*
    };
}

// you can leave funct3 and funct7 any value if this format don't need it
// TODO: make funct3 and funct7 optional
define_riscv_isa!(
    Riscv32Instr,
    RV32I, TABLE_RV32I, {
        ADDI {
            opcode: 0b0010011,
            funct3: 0b000,
            funct7: 0b0000000,
            format: InstrFormat::I,
        },
        ADD {
            opcode: 0b0110011,
            funct3: 0b000,
            funct7: 0b0000000,
            format: InstrFormat::R,
        },
        SUB {
            opcode: 0b0110011,
            funct3: 0b000,
            funct7: 0b0100000,
            format: InstrFormat::R,
        },
    },
);

#[derive(Debug, Clone)]
pub enum Exception {
    InvalidInstruction,
}
