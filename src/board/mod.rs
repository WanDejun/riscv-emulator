use crate::isa::{ISATypes, riscv::trap::Exception};

pub mod virt;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BoardStatus {
    Running,
    Halt,
}

pub trait Board {
    type ISA: ISATypes;

    fn step(&mut self) -> Result<(), Exception>;
    fn status(&self) -> BoardStatus;

    fn cpu_mut(&mut self) -> &mut <Self::ISA as ISATypes>::CPU;
}
