pub mod address;
pub mod config;
mod page_table;

use std::{cell::UnsafeCell, rc::Rc};

use crate::{
    config::arch_config::WordType,
    device::{DeviceTrait, Mem, MemError, mmio::MemoryMapIO},
    isa::riscv::{
        csr_reg::{
            CsrRegFile, PrivilegeLevel,
            csr_macro::{Mstatus, Sstatus},
        },
        debugger::Address,
        mmu::{
            config::PAGE_SIZE_XLEN,
            page_table::{PTEFlags, PageTable, PageTableError},
        },
        trap::Exception,
    },
    ram::Ram,
    utils::UnsignedInteger,
};

enum MemAccessView {
    UserOnly,          // only U-flag
    SupervisorOnly,    // only NO U-flag
    SupervisorAndUser, // ignore U-flag
    MachineOnly,       // physical address directly
}

impl MemAccessView {
    fn new(csr: &mut CsrRegFile) -> Self {
        match csr.privelege_level() {
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

    pub(crate) fn read<T>(&mut self, addr: WordType, csr: &mut CsrRegFile) -> Result<T, MemError>
    where
        T: UnsignedInteger,
    {
        let view = MemAccessView::new(csr);
        let (masks, flags) = match view {
            MemAccessView::MachineOnly => return self.mmio.read(addr.into()),
            MemAccessView::SupervisorAndUser => (PTEFlags::R, PTEFlags::R),
            MemAccessView::SupervisorOnly => (PTEFlags::R | PTEFlags::U, PTEFlags::R),
            MemAccessView::UserOnly => (PTEFlags::R | PTEFlags::U, PTEFlags::R | PTEFlags::U),
        };

        if let Ok(paddr) = self.translate_vaddr::<false, true>(addr, masks, flags) {
            self.mmio.read(paddr)
        } else {
            return Err(MemError::LoadPageFault);
        }
    }

    pub(crate) fn write<T>(
        &mut self,
        addr: WordType,
        data: T,
        csr: &mut CsrRegFile,
    ) -> Result<(), MemError>
    where
        T: UnsignedInteger,
    {
        let view = MemAccessView::new(csr);
        let (masks, flags) = match view {
            MemAccessView::MachineOnly => return self.mmio.write(addr.into(), data),
            MemAccessView::SupervisorAndUser => (PTEFlags::W, PTEFlags::W),
            MemAccessView::SupervisorOnly => (PTEFlags::W | PTEFlags::U, PTEFlags::W),
            MemAccessView::UserOnly => (PTEFlags::W | PTEFlags::U, PTEFlags::W | PTEFlags::U),
        };

        if let Ok(paddr) = self.translate_vaddr::<true, true>(addr, masks, flags) {
            self.mmio.write(paddr, data)
        } else {
            Err(MemError::StorePageFault)
        }
    }

    fn ifetch_impl<const ACCESS: bool, T>(
        &mut self,
        addr: WordType,
        csr: &mut CsrRegFile,
    ) -> Result<T, MemError>
    where
        T: UnsignedInteger,
    {
        let privilege = csr.privelege_level();

        let (masks, flags) = match privilege {
            PrivilegeLevel::M => return self.mmio.read(addr.into()),
            PrivilegeLevel::S => (PTEFlags::X, PTEFlags::X),
            PrivilegeLevel::U => (PTEFlags::X | PTEFlags::U, PTEFlags::X | PTEFlags::U),
            PrivilegeLevel::V => unreachable!(), // Doesn't have V-mode.
        };

        if let Ok(paddr) = self.translate_vaddr::<false, true>(addr, masks, flags) {
            self.mmio.read(paddr)
        } else {
            Err(MemError::LoadPageFault)
        }
    }

    pub(crate) fn ifetch<T>(&mut self, addr: WordType, csr: &mut CsrRegFile) -> Result<T, MemError>
    where
        T: UnsignedInteger,
    {
        self.ifetch_impl::<true, _>(addr, csr)
    }

    /// Fetch instruction without side-effect, respecting the privilege mode.
    ///
    /// Provided for debugger.
    pub(crate) fn debug_ifetch<T>(
        &mut self,
        addr: WordType,
        csr: &mut CsrRegFile,
    ) -> Result<T, MemError>
    where
        T: UnsignedInteger,
    {
        self.ifetch_impl::<false, _>(addr, csr)
    }

    /// do some binary operation on memory by callback function.
    /// if rhs_addr is 0, unary operation instead.
    pub(crate) fn modify_mem_by<F, T>(
        &mut self,
        _lhs_addr: WordType,
        _rhs_addr: WordType,
        _f: F,
    ) -> Result<T, Exception>
    where
        T: UnsignedInteger,
        F: Fn(&T::AtomicType, &T::AtomicType) -> Result<T, Exception>,
    {
        todo!();
    }

    pub(crate) fn read_by_paddr<T>(&mut self, paddr: WordType) -> Result<T, MemError>
    where
        T: UnsignedInteger,
    {
        self.mmio.read(paddr.into())
    }

    pub(crate) fn write_by_paddr<T>(&mut self, paddr: WordType, data: T) -> Result<(), MemError>
    where
        T: UnsignedInteger,
    {
        self.mmio.write(paddr.into(), data)
    }

    /// Read operation without side-effect of page table, provided for debugger.
    ///
    /// This function dones't respect the current privilege mode. See also `get_instr_code_pure`.
    pub(crate) fn debug_read<T>(&mut self, addr: Address) -> Result<T, MemError>
    where
        T: UnsignedInteger,
    {
        match addr {
            Address::Phys(addr) => self.read_by_paddr::<T>(addr),
            Address::Virt(addr) => {
                let (masks, flags) = (PTEFlags::empty(), PTEFlags::empty());

                if let Ok(paddr) = self.translate_vaddr::<false, false>(addr, masks, flags) {
                    self.mmio.read(paddr)
                } else {
                    Err(MemError::LoadPageFault)
                }
            }
        }
    }

    /// Write operation without side-effect of page table, provided for debugger.
    pub(crate) fn debug_write<T>(&mut self, addr: Address, data: T) -> Result<(), MemError>
    where
        T: UnsignedInteger,
    {
        match addr {
            Address::Phys(addr) => self.write_by_paddr::<T>(addr, data),
            Address::Virt(addr) => {
                let (masks, flags) = (PTEFlags::empty(), PTEFlags::empty());

                if let Ok(paddr) = self.translate_vaddr::<false, false>(addr, masks, flags) {
                    self.mmio.write(paddr, data)
                } else {
                    Err(MemError::StorePageFault)
                }
            }
        }
    }

    fn translate_vaddr<const DIRTY: bool, const ACCESS: bool>(
        &mut self,
        vaddr: WordType,
        masks: PTEFlags,
        target_flags: PTEFlags,
    ) -> Result<u64, PageTableError> {
        self.page_table
            .translate_vaddr::<DIRTY, ACCESS>(
                unsafe { self.ram.as_mut_unchecked() },
                vaddr.into(),
                masks,
                target_flags,
            )
            .map(|addr| addr.into())
    }

    /// Translate virtual address to physical address without side-effect.
    ///
    /// NOTE: This function donesn't respect privilege level.
    pub(crate) fn debug_translate_vaddr(&self, vaddr: WordType) -> Result<u64, PageTableError> {
        let (masks, flags) = (PTEFlags::empty(), PTEFlags::empty());

        self.page_table
            .translate_vaddr::<false, false>(
                unsafe { self.ram.as_mut_unchecked() },
                vaddr.into(),
                masks,
                flags,
            )
            .map(|paddr| paddr.0)
    }

    // virtual memory
    pub fn set_mode(&mut self, mode: u8) {
        self.page_table.set_mode(mode);
    }

    pub fn set_root_ppn(&mut self, ppn: u64) {
        self.page_table.set_root_addr(ppn << PAGE_SIZE_XLEN);
    }

    // sync mmio devices
    pub fn sync(&mut self) {
        self.mmio.sync();
    }
}
