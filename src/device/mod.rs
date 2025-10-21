use crate::{
    async_poller::PollingEvent, config::arch_config::WordType,
    device::fast_uart::FastUart16550Handle, handle_trait::HandleTrait, utils::UnsignedInteger,
};

pub(crate) mod aclint;
pub(crate) mod config;
pub mod fast_uart;
mod id_allocator;
pub(crate) use id_allocator::*;
pub(crate) mod mmio;
pub(crate) mod power_manager;
pub(crate) mod virtio;

#[derive(Debug, PartialEq, Eq)]
pub enum MemError {
    LoadPageFault,
    LoadMisaligned,
    LoadFault,
    StorePageFault,
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
    fn get_poll_enent(&mut self) -> Option<PollingEvent>;
}

pub trait MemMappedDeviceTrait: DeviceTrait {
    fn base() -> WordType;
    fn size() -> WordType;
}

// / Peripheral initialization
pub fn peripheral_init() -> Vec<Box<dyn HandleTrait>> {
    return vec![Box::new(FastUart16550Handle::new())];
}
