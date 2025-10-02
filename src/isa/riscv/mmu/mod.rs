pub mod address;
pub mod config;
mod page_table;

use std::{cell::UnsafeCell, rc::Rc};

use crate::{
    device::{DeviceTrait, Mem, MemError, mmio::MemoryMapIO},
    isa::riscv::{
        csr_reg::{
            CsrRegFile, PrivilegeLevel,
            csr_macro::{Mstatus, Sstatus},
        },
        mmu::{
            config::PAGE_SIZE_XLEN,
            page_table::{PTEFlags, PageTable},
        },
    },
    ram::Ram,
};

enum MemAccessView {
    UserOnly,          // only U-flag
    SupervisorOnly,    // only NO U-flag
    SupervisorAndUser, // ignore U-flag
    MachineOnly,       // physical address directly
}

impl MemAccessView {
    fn new(csr: &mut CsrRegFile) -> Self {
        match csr.get_current_privilege() {
            PrivilegeLevel::M => {
                let mstatus = csr.get_by_type_existing::<Mstatus>();

                if mstatus.get_mprv() == 0 {
                    Self::MachineOnly
                } else {
                    match (mstatus.get_mpp() as u8).try_into().unwrap() {
                        PrivilegeLevel::M => Self::MachineOnly,
                        PrivilegeLevel::S => {
                            if mstatus.get_sum() == 0 {
                                Self::SupervisorOnly
                            } else {
                                Self::SupervisorAndUser
                            }
                        }
                        PrivilegeLevel::U => Self::UserOnly,
                        PrivilegeLevel::V => todo!(), // Doesn't have V-mode.
                    }
                }
            }
            PrivilegeLevel::S => {
                let sstatus = csr.get_by_type_existing::<Sstatus>();

                if sstatus.get_sum() == 0 {
                    Self::SupervisorOnly
                } else {
                    Self::SupervisorAndUser
                }
            }
            PrivilegeLevel::U => Self::UserOnly,
            PrivilegeLevel::V => unreachable!(), // Doesn't have V-mode.
        }
    }
}

pub(crate) struct VirtAddrManager {
    mmio: MemoryMapIO,
    page_table: PageTable,
    ram: Rc<UnsafeCell<Ram>>,
}

impl VirtAddrManager {
    pub(crate) fn from_ram_and_mmio(ram_ref: Rc<UnsafeCell<Ram>>, mmio: MemoryMapIO) -> Self {
        Self {
            mmio: mmio,
            page_table: PageTable::new(0, config::VirtualMemoryMode::None),
            ram: ram_ref,
        }
    }

    pub(crate) fn read<T>(
        &mut self,
        addr: crate::config::arch_config::WordType,
        csr: &mut CsrRegFile,
    ) -> Result<T, MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        let view = MemAccessView::new(csr);
        let masks;
        let flags;

        match view {
            MemAccessView::MachineOnly => return self.mmio.read(addr.into()),
            MemAccessView::SupervisorAndUser => {
                (masks, flags) = (PTEFlags::R, PTEFlags::R);
            }
            MemAccessView::SupervisorOnly => {
                (masks, flags) = (PTEFlags::R | PTEFlags::U, PTEFlags::R);
            }
            MemAccessView::UserOnly => {
                (masks, flags) = (PTEFlags::R | PTEFlags::U, PTEFlags::R | PTEFlags::U);
            }
        }

        if let Ok(paddr) = self.page_table.translate_addr::<false, true>(
            unsafe { self.ram.as_mut_unchecked() },
            addr.into(),
            masks,
            flags,
        ) {
            self.mmio.read(paddr.into())
        } else {
            return Err(MemError::LoadPageFault);
        }
    }

    pub(crate) fn write<T>(
        &mut self,
        addr: crate::config::arch_config::WordType,
        data: T,
        csr: &mut CsrRegFile,
    ) -> Result<(), MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        let view = MemAccessView::new(csr);
        let masks;
        let flags;

        match view {
            MemAccessView::MachineOnly => return self.mmio.write(addr.into(), data),
            MemAccessView::SupervisorAndUser => {
                (masks, flags) = (PTEFlags::W, PTEFlags::W);
            }
            MemAccessView::SupervisorOnly => {
                (masks, flags) = (PTEFlags::W | PTEFlags::U, PTEFlags::W);
            }
            MemAccessView::UserOnly => {
                (masks, flags) = (PTEFlags::W | PTEFlags::U, PTEFlags::W | PTEFlags::U);
            }
        }

        if let Ok(paddr) = self.page_table.translate_addr::<true, true>(
            unsafe { self.ram.as_mut_unchecked() },
            addr.into(),
            masks,
            flags,
        ) {
            self.mmio.write(paddr.into(), data)
        } else {
            Err(MemError::StorePageFault)
        }
    }

    pub(crate) fn get_instr_code<T>(
        &mut self,
        addr: crate::config::arch_config::WordType,
        csr: &mut CsrRegFile,
    ) -> Result<T, MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        let privilege = csr.get_current_privilege();
        let masks;
        let flags;

        match privilege {
            PrivilegeLevel::M => return self.mmio.read(addr.into()),
            PrivilegeLevel::S => {
                (masks, flags) = (PTEFlags::X, PTEFlags::X);
            }
            PrivilegeLevel::U => {
                (masks, flags) = (PTEFlags::X | PTEFlags::U, PTEFlags::X | PTEFlags::U);
            }
            PrivilegeLevel::V => unreachable!(), // Doesn't have V-mode.
        }

        if let Ok(paddr) = self.page_table.translate_addr::<false, true>(
            unsafe { self.ram.as_mut_unchecked() },
            addr.into(),
            masks,
            flags,
        ) {
            self.mmio.read(paddr.into())
        } else {
            Err(MemError::LoadPageFault)
        }
    }

    pub(crate) fn read_by_paddr<T>(
        &mut self,
        paddr: crate::config::arch_config::WordType,
    ) -> Result<T, MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        self.mmio.read(paddr.into())
    }

    pub(crate) fn write_by_paddr<T>(
        &mut self,
        paddr: crate::config::arch_config::WordType,
        data: T,
    ) -> Result<(), MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        self.mmio.write(paddr.into(), data)
    }

    pub(crate) fn get_instr_code_without_side_effect<T>(
        &mut self,
        addr: crate::config::arch_config::WordType,
        csr: &mut CsrRegFile,
    ) -> Result<T, MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        let privilege = csr.get_current_privilege();
        let masks;
        let flags;

        match privilege {
            PrivilegeLevel::M => return self.mmio.read(addr.into()),
            PrivilegeLevel::S => {
                (masks, flags) = (PTEFlags::X, PTEFlags::X);
            }
            PrivilegeLevel::U => {
                (masks, flags) = (PTEFlags::X | PTEFlags::U, PTEFlags::X | PTEFlags::U);
            }
            PrivilegeLevel::V => unreachable!(), // Doesn't have V-mode.
        }

        if let Ok(paddr) = self.page_table.translate_addr::<false, false>(
            unsafe { self.ram.as_mut_unchecked() },
            addr.into(),
            masks,
            flags,
        ) {
            self.mmio.read(paddr.into())
        } else {
            Err(MemError::LoadPageFault)
        }
    }

    pub fn set_mode(&mut self, mode: u8) {
        self.page_table.set_mode(mode);
    }

    pub fn set_root_ppn(&mut self, ppn: u64) {
        self.page_table.set_root_addr(ppn << PAGE_SIZE_XLEN);
    }

    pub fn sync(&mut self) {
        self.mmio.sync();
    }
}
