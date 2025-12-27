use crate::isa::riscv::{executor::RVCPU, trap::Exception};

pub mod virt;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BoardStatus {
    Running,
    Halt,
}

pub trait Board {
    fn step(&mut self) -> Result<(), Exception>;
    fn status(&self) -> BoardStatus;

    fn cpu(&self) -> &RVCPU;
    fn cpu_mut(&mut self) -> &mut RVCPU;

    fn loader(&self) -> Option<&crate::load::ELFLoader>;
}
