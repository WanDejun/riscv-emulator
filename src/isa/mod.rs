use std::fmt::Debug;

use crate::{config::arch_config::WordType, device::MemError, utils::UnsignedInteger};

pub mod icache;
pub mod riscv;

mod utils;

pub trait DebugTarget<I: ISATypes> {
    fn read_pc(&self) -> WordType;
    fn write_pc(&mut self, new_pc: WordType);

    fn read_instr(&mut self, addr: WordType) -> Result<I::RawInstr, MemError>;
    fn write_back_instr(&mut self, instr: I::RawInstr, addr: WordType) -> Result<(), MemError>;

    fn read_reg(&self, idx: u8) -> WordType;
    fn write_reg(&mut self, idx: u8, value: WordType);

    fn read_float_reg(&self, idx: u8) -> f64;

    fn read_mem<T: UnsignedInteger>(&mut self, addr: WordType) -> Result<T, MemError>;
    fn write_mem<T: UnsignedInteger>(&mut self, addr: WordType, data: T) -> Result<(), MemError>;

    fn debug_csr(&mut self, addr: WordType, new_value: Option<WordType>) -> Option<WordType>;

    fn step(&mut self) -> Result<(), I::StepException>;

    fn decoded_info(&mut self, addr: I::RawInstr) -> Option<I::DecodeRst>;
}

pub trait DecoderTrait<I: ISATypes> {
    fn from_isa(instrs: &[I::ISADesc]) -> Self;
    fn decode(&self, instr: I::RawInstr) -> Option<I::DecodeRst>;
}

pub trait HasBreakpointException {
    fn is_breakpoint(&self) -> bool;
}

pub trait InstrLen {
    fn len(&self) -> WordType;
}

pub trait ISATypes: Sized {
    const EBREAK: Self::RawInstr;

    type RawInstr: Copy + InstrLen;
    type ISADesc;
    type DecodeRst: Clone + Copy;
    type StepException: HasBreakpointException + Debug;
    type Decoder: DecoderTrait<Self>;
    type CPU: DebugTarget<Self>;
}

impl InstrLen for u32 {
    fn len(&self) -> WordType {
        4
    }
}
