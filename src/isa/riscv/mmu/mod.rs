pub mod address;
pub mod config;
mod page_table;

use std::{cell::UnsafeCell, rc::Rc};

use crate::{
    device::{DeviceTrait, Mem, MemError, mmio::MemoryMapIO},
    ram::Ram,
};

pub struct VirtAddrManager {
    mmio: MemoryMapIO,
    ram: Rc<UnsafeCell<Ram>>,
}

impl VirtAddrManager {
    pub fn new() -> Self {
        Self::from_ram(Ram::new())
    }

    pub fn from_ram(ram: Ram) -> Self {
        let ram_ref = Rc::new(UnsafeCell::new(ram));
        Self {
            mmio: MemoryMapIO::from_ram(ram_ref.clone()),
            ram: ram_ref,
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
