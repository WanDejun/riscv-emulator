pub(super) mod exec_function;
pub mod rv32i_table;

use crate::config::arch_config::WordType;

/// `imm` values saved should be shifted, like B, U and J type.
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
#[macro_export]
macro_rules! define_riscv_isa {
    ( $tot_instr_name:ident,
        $( $isa_name:ident, $isa_table_name:ident, {$(
                $name:ident {
                    opcode: $opcode:literal,
                    funct3: $funct3:literal,
                    funct7: $funct7:literal,
                    format: $fmt:expr,
                    callback: $callback: expr,
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
            // pub callback: fn(RVInstrInfo, &mut RV32CPU) -> Result<(), Exception>,
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
                        // callback: $callback,
                    }
                ),*
            ];
        )*

        pub(in crate::isa::riscv32) fn get_exec_func(
            instr: $tot_instr_name
        ) -> fn(RVInstrInfo, &mut RV32CPU) -> Result<(), Exception> {
            match instr {
                $($($tot_instr_name::$name => $callback),*),*
            }
        }
    };
}

#[derive(Debug, Clone)]
pub enum Exception {
    InvalidInstruction,
}

// call [`define_riscv_isa!`] to generate instructions
// include!(concat!(env!("OUT_DIR"), "/rv32i_gen.rs"));
