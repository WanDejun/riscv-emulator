pub(super) mod exec_function;
pub mod exec_mapping;
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
                    mask: $mask:literal,
                    key: $key:literal,
                    use_mask: $use_mask:literal,
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
            pub mask: u32,
            pub key: u32,
            pub use_mask: bool,
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
                        mask: $mask,
                        key: $key,
                        use_mask: $use_mask,
                    }
                ),*
            ];
        )*
    };
}

// call [`define_riscv_isa!`] to generate instructions
// include!(concat!(env!("OUT_DIR"), "/rv32i_gen.rs"));
