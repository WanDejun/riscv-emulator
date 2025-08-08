use crate::{config::arch_config::WordType, utils::UnsignedInteger};
pub mod uart;

// Check align requirement before device.read/write. Most of align requirement was checked in mmio.
pub trait DeviceTrait {
    fn read<T>(&mut self, inner_addr: WordType) -> T
    where
        T: UnsignedInteger;
    fn write<T>(&mut self, inner_addr: WordType, data: T)
    where
        T: UnsignedInteger;
}
