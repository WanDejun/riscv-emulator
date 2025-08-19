use crate::isa::{
    ISATypes,
    riscv::{decoder::DecodeInstr, executor::RV32CPU, instruction::rv32i_table::RV32Desc},
};

mod cpu_tester;
pub mod csr_reg;
pub mod debugger;
pub mod decoder;
pub mod executor;
pub mod instruction;
pub mod trap;
pub mod vaddr;

#[derive(Debug)]
pub struct RiscvTypes;

impl ISATypes for RiscvTypes {
    const EBREAK: u32 = 0x00100073;

    type RawInstr = u32;
    type ISADesc = RV32Desc;
    type DecodeRst = DecodeInstr;
    type StepException = trap::Exception;
    type Decoder = decoder::Decoder;
    type CPU = RV32CPU;
}
