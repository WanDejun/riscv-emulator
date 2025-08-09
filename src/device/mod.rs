use crate::{config::arch_config::WordType, utils::UnsignedInteger};
mod config;
pub mod mmio;
pub mod uart;
pub mod cli;

pub trait Mem {
    fn read<T>(&mut self, addr: WordType) -> T
    where
        T: UnsignedInteger;

    fn write<T>(&mut self, addr: WordType, data: T)
    where
        T: UnsignedInteger;
}

// Check align requirement before device.read/write. Most of align requirement was checked in mmio.
pub trait DeviceTrait: Mem {
    fn one_shot(&mut self);
}
