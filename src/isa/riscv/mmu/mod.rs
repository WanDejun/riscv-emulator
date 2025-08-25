pub mod address;
pub mod config;
mod page_table;

use crate::{
    device::{DeviceTrait, Mem, MemError, mmio::MemoryMapIO},
    ram::Ram,
};

pub struct VirtAddrManager {
    mmio: MemoryMapIO,
}

impl VirtAddrManager {
    pub fn new() -> Self {
        Self {
            mmio: MemoryMapIO::new(),
        }
    }

    pub fn from_ram(ram: Ram) -> Self {
        Self {
            mmio: MemoryMapIO::from_ram(ram),
        }
    }
}

impl Mem for VirtAddrManager {
    fn read<T>(&mut self, addr: crate::config::arch_config::WordType) -> Result<T, MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        self.mmio.read(addr)
    }

    fn write<T>(
        &mut self,
        addr: crate::config::arch_config::WordType,
        data: T,
    ) -> Result<(), MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        self.mmio.write(addr, data)
    }
}

impl DeviceTrait for VirtAddrManager {
    fn sync(&mut self) {
        self.mmio.sync();
    }
}
