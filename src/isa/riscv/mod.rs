use crate::isa::{
    ISATypes,
    riscv::{decoder::DecodeInstr, executor::RVCPU, instruction::instr_table::RVInstrDesc},
};

mod cpu_tester;
pub mod csr_reg;
pub mod debugger;
pub mod decoder;
pub mod executor;
pub mod instruction;
pub mod mmu;
pub mod trap;

#[derive(Debug)]
pub struct RiscvTypes;

impl ISATypes for RiscvTypes {
    const EBREAK: u32 = 0x00100073;

    type RawInstr = u32;
    type ISADesc = RVInstrDesc;
    type DecodeRst = DecodeInstr;
    type StepException = trap::Exception;
    type Decoder = decoder::Decoder;
    type CPU = RVCPU;
}
