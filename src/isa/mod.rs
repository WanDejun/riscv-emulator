use std::fmt::Debug;

use crate::{
    config::arch_config::WordType,
    device::MemError,
    isa::riscv::{csr_reg::PrivilegeLevel, debugger::Address},
    utils::UnsignedInteger,
};

pub mod icache;
pub mod riscv;

mod utils;

pub trait DebugTarget<I: ISATypes> {
    fn read_pc(&self) -> WordType;
    fn write_pc(&mut self, new_pc: WordType);

    fn read_reg(&self, idx: u8) -> WordType;
    fn write_reg(&mut self, idx: u8, value: WordType);
    fn read_float_reg(&self, idx: u8) -> (f32, f64);

    fn read_instr(&mut self, addr: WordType) -> Result<I::RawInstr, MemError>;
    fn read_instr_directly(&mut self, addr: Address) -> Result<I::RawInstr, MemError>;

    fn read_memory<T: UnsignedInteger>(&mut self, addr: Address) -> Result<T, MemError>;
    fn write_memory<T: UnsignedInteger>(&mut self, addr: Address, data: T) -> Result<(), MemError>;

    fn vaddr_to_paddr(&self, vaddr: WordType) -> Option<u64>;
    /// This function respect the privilege level.
    fn translate(&self, addr: WordType) -> Option<u64>;

    fn get_current_privilege(&self) -> PrivilegeLevel;

    fn debug_csr(&mut self, addr: WordType, new_value: Option<WordType>) -> Option<WordType>;

    fn step(&mut self) -> Result<(), I::StepException>;

    fn decoded_instr(&self, instr: I::RawInstr) -> Option<I::DecodeRst>;
}

pub trait DecoderTrait<I: ISATypes> {
    fn from_isa(instrs: &[I::ISADesc]) -> Self;
    fn decode(&self, instr: I::RawInstr) -> Option<I::DecodeRst>;
}

pub trait InstrLen {
    fn len(&self) -> WordType;
}

pub trait ISATypes: Sized {
    const EBREAK: Self::RawInstr;

    type RawInstr: Copy + InstrLen;
    type ISADesc;
    type DecodeRst: Clone + Copy;
    type StepException: Debug;
    type Decoder: DecoderTrait<Self>;
    type CPU: DebugTarget<Self>;
}

impl InstrLen for u32 {
    fn len(&self) -> WordType {
        4
    }
}
