pub mod address;
pub mod config;
mod page_table;

use std::{cell::UnsafeCell, rc::Rc};

use crate::{
    device::{DeviceTrait, Mem, MemError, mmio::MemoryMapIO},
    isa::riscv::mmu::{
        config::PAGE_SIZE,
        page_table::{PTEFlags, PageTable},
    },
    ram::Ram,
};

pub struct VirtAddrManager {
    mmio: MemoryMapIO,
    page_table: PageTable,
    ram: Rc<UnsafeCell<Ram>>,
}

impl VirtAddrManager {
    pub fn from_ram_and_mmio(ram_ref: Rc<UnsafeCell<Ram>>, mmio: MemoryMapIO) -> Self {
        Self {
            mmio: mmio,
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

    pub fn set_mode(&mut self, mode: u8) {
        self.page_table.set_mode(mode);
    }

    pub fn set_root_ppn(&mut self, ppn: u64) {
        // FIXME: This function actually accepts address (ppn * PAGE_SIZE) due to its chaos design.
        self.page_table
            .set_root_ppn_by_addr((ppn * PAGE_SIZE).into());
    }

    pub fn sync(&mut self) {
        self.mmio.sync();
    }
}
