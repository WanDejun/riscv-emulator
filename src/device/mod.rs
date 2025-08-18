#[cfg(not(test))]
use crate::device::cli_uart::CliUart;
#[cfg(test)]
use crate::device::cli_uart::FIFOUart;

use crate::{
    config::arch_config::WordType, device::cli_uart::CliUartHandle, handle_trait::HandleTrait,
    utils::UnsignedInteger,
};
pub mod cli_uart;
mod config;
pub mod mmio;
pub mod power_manager;
pub mod uart;

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
    fn step(&mut self);
    fn sync(&mut self);
}

#[cfg(test)]
type DebugUart = FIFOUart;
#[cfg(not(test))]
type DebugUart = CliUart;

// / Peripheral initialization
pub fn peripheral_init() -> Vec<Box<dyn HandleTrait>> {
    return vec![Box::new(CliUartHandle::new())];
}
