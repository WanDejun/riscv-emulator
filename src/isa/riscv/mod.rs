use crate::{
    config::arch_config::WordType,
    isa::{
        ISATypes, InstrLen,
        riscv::{decoder::DecodeInstr, executor::RVCPU, instruction::instr_table::RVInstrDesc},
    },
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
    type RawInstr = RawInstr;
    type ISADesc = RVInstrDesc;
    type DecodeRst = DecodeInstr;
    type StepException = trap::Exception;
    type CPU = RVCPU;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawInstr {
    pub val: u32,
}

impl From<u32> for RawInstr {
    fn from(value: u32) -> Self {
        Self { val: value }
    }
}

impl InstrLen for RawInstr {
    fn len(&self) -> WordType {
        if self.val & 0b11 == 0b11 { 4 } else { 2 }
    }
}
