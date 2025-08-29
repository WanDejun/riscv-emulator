pub mod address;
pub mod config;
mod page_table;

use std::{cell::UnsafeCell, rc::Rc};

use crate::{
    device::{DeviceTrait, Mem, MemError, mmio::MemoryMapIO},
    isa::riscv::mmu::page_table::{PTEFlags, PageTable},
    ram::Ram,
};

pub struct VirtAddrManager {
    mmio: MemoryMapIO,
    page_table: PageTable,
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
            page_table: PageTable::new(0.into(), config::VirtualMemoryMode::None),
            ram: ram_ref,
        }
    }
    pub fn read<T>(&mut self, addr: crate::config::arch_config::WordType) -> Result<T, MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        if let Ok(paddr) = self.page_table.translate_addr(
            unsafe { self.ram.as_mut_unchecked() },
            addr.into(),
            PTEFlags::R,
        ) {
            self.mmio.read(paddr.into())
        } else {
            Err(MemError::LoadPageFault)
        }
    }

    pub fn write<T>(
        &mut self,
        addr: crate::config::arch_config::WordType,
        data: T,
    ) -> Result<(), MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        if let Ok(paddr) = self.page_table.translate_addr(
            unsafe { self.ram.as_mut_unchecked() },
            addr.into(),
            PTEFlags::W,
        ) {
            self.mmio.write(paddr.into(), data)
        } else {
            Err(MemError::StorePageFault)
        }
    }

    pub fn get_instr_code<T>(
        &mut self,
        addr: crate::config::arch_config::WordType,
        data: T,
    ) -> Result<(), MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        if let Ok(paddr) = self.page_table.translate_addr(
            unsafe { self.ram.as_mut_unchecked() },
            addr.into(),
            PTEFlags::X,
        ) {
            self.mmio.write(paddr.into(), data)
        } else {
            Err(MemError::StorePageFault)
        }
    }

    pub fn sync(&mut self) {
        self.mmio.sync();
    }
}
