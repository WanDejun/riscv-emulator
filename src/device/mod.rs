use crate::{
    config::arch_config::WordType, device::fast_uart::FastUart16550Handle,
    handle_trait::HandleTrait, utils::UnsignedInteger,
};
mod config;
pub mod fast_uart;
pub mod mmio;
pub mod power_manager;

// TODO: Improve error info
#[derive(Debug, PartialEq, Eq)]
pub enum MemError {
    LoadMisaligned,
    LoadFault,
    StoreMisaligned,
    StoreFault,
}

pub trait Mem {
    fn read<T>(&mut self, addr: WordType) -> Result<T, MemError>
    where
        T: UnsignedInteger;

    fn write<T>(&mut self, addr: WordType, data: T) -> Result<(), MemError>
    where
        T: UnsignedInteger;
}

// Check align requirement before device.read/write. Most of align requirement was checked in mmio.
pub trait DeviceTrait: Mem {
    fn sync(&mut self);
}

// / Peripheral initialization
pub fn peripheral_init() -> Vec<Box<dyn HandleTrait>> {
    return vec![Box::new(FastUart16550Handle::new())];
}
