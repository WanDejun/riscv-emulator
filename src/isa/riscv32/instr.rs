type ExecuteFn = fn(); // TODO

#[derive(Debug, Clone, PartialEq)]
pub enum RVInstrInfo {
    R { rs1: u8, rs2: u8, rd: u8 },
    I { rs1: u8, rd: u8, imm: u32 },
    S { rs1: u8, rs2: u8, imm: u32 },
    B { rs1: u8, rs2: u8, imm: u32 },
    U { rd: u8, imm: u32 },
    J { rd: u8, imm: u32 },
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

use crate::define_instr_enum;

// define a single enum for every instruction
// define tables for each instruction set
macro_rules! define_riscv_isa {
    ( $tot_instr_name:ident,
        $( $isa_name:ident, $isa_table_name:ident, {$(
                $name:ident {
                    opcode: $opcode:literal,
                    func3: $func3:literal,
                    func7: $func7:literal,
                    format: $fmt:expr,
                    execute: $execute:expr,
                }),* $(,)?
            }
        ),* $(,)?
    ) => {

        define_instr_enum!($tot_instr_name, $($($name,)*)*);

        $(
            pub const $isa_table_name: &[(u8, u8, u8, $tot_instr_name, InstrFormat, ExecuteFn)] = &[
                $(
                    (
                        $opcode, $func3, $func7,
                        $tot_instr_name::$name,
                        $fmt,
                        $execute,
                    )
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
            func3: 0b000,
            func7: 0b0000000,
            format: InstrFormat::I,
            execute: || { todo!() },
        },
        ADD {
            opcode: 0b0110011,
            func3: 0b000,
            func7: 0b0000000,
            format: InstrFormat::R,
            execute: || { todo!() },
        },
        SUB {
            opcode: 0b0110011,
            func3: 0b000,
            func7: 0b0100000,
            format: InstrFormat::R,
            execute: || { todo!() },
        },
    },
);

#[derive(Debug, Clone)]
pub enum Exception {
    InvalidInstruction,
}
