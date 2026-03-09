pub mod address;
pub mod config;
mod page_table;

pub use page_table::PageTableError;

use std::{cell::UnsafeCell, rc::Rc};

use crate::{
    config::arch_config::WordType,
    device::{DeviceTrait, MemError, mmio::MemoryMapIO},
    isa::riscv::{
        csr_reg::{
            CsrRegFile, PrivilegeLevel,
            csr_macro::{Mstatus, Sstatus},
        },
        debugger::Address,
        mmu::{
            config::{AccessEffect, AdUpdatePolicy, PAGE_SIZE_XLEN},
            page_table::{PTEFlags, PageTable},
        },
        trap::Exception,
    },
    ram::Ram,
    ram_config,
    utils::UnsignedInteger,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AccessType {
    Read,
    Write,
    // TODO: Consider Remove this. We don't have to handle AMO in this way.
    ReadWrite,
    // TODO: Currently we handle ifetch separately, consider adding ifetch here so that we can unify the handling.
}

enum AccessPolicy {
    Direct,
    Translated {
        masks: PTEFlags,
        flags: PTEFlags,
        effect: AccessEffect,
        fault: MemError,
    },
}

enum AccessPrivilege {
    /// only U-flag
    UserOnly,

    /// only S-flag
    SupervisorOnly,

    /// ignore U-flag in S mode, except ifetch.
    SupervisorAndUser,

    /// physical address directly
    MachineOnly,
}

/// Determine in which mode the data access should be performed, and which PTE flags should be checked,
/// based on the current privilege level and relevant CSR settings.
fn determine_data_access_privilege(csr: &mut CsrRegFile) -> AccessPrivilege {
    match csr.privelege_level() {
        PrivilegeLevel::M => {
            let mstatus = csr.get_by_type_existing::<Mstatus>();

            if mstatus.get_mprv() == 0 {
                AccessPrivilege::MachineOnly
            } else {
                match (mstatus.get_mpp() as u8).into() {
                    PrivilegeLevel::M => AccessPrivilege::MachineOnly,
                    PrivilegeLevel::S => {
                        if mstatus.get_sum() == 0 {
                            AccessPrivilege::SupervisorOnly
                        } else {
                            AccessPrivilege::SupervisorAndUser
                        }
                    }
                    PrivilegeLevel::U => AccessPrivilege::UserOnly,
                    PrivilegeLevel::V => todo!(), // Doesn't have V-mode.
                }
            }
        }
        PrivilegeLevel::S => {
            let sstatus = csr.get_by_type_existing::<Sstatus>();

            if sstatus.get_sum() == 0 {
                AccessPrivilege::SupervisorOnly
            } else {
                AccessPrivilege::SupervisorAndUser
            }
        }
        PrivilegeLevel::U => AccessPrivilege::UserOnly,
        PrivilegeLevel::V => unreachable!(), // Doesn't have V-mode.
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

    /// NOTE: This function only resolves data access, for ifetch, please use `resolve_ifetch_policy`.
    #[inline]
    fn resolve_data_policy(
        csr: &mut CsrRegFile,
        access: AccessType,
        side_effect: bool,
    ) -> AccessPolicy {
        let fault = match access {
            AccessType::Read => MemError::LoadPageFault,
            AccessType::Write | AccessType::ReadWrite => MemError::StorePageFault,
        };

        let effect = match (side_effect, access) {
            (false, _) => AccessEffect::None,
            (true, AccessType::Read) => AccessEffect::Accessed,
            (true, AccessType::Write | AccessType::ReadWrite) => AccessEffect::AccessedDirty,
        };

        let rwx_base = match access {
            AccessType::Read => PTEFlags::R,
            AccessType::Write => PTEFlags::W,
            AccessType::ReadWrite => PTEFlags::R | PTEFlags::W,
        };

        let (masks, flags) = match determine_data_access_privilege(csr) {
            AccessPrivilege::MachineOnly => return AccessPolicy::Direct,

            AccessPrivilege::SupervisorAndUser => (rwx_base, rwx_base),
            AccessPrivilege::SupervisorOnly => (rwx_base | PTEFlags::U, rwx_base),
            AccessPrivilege::UserOnly => (rwx_base | PTEFlags::U, rwx_base | PTEFlags::U),
        };

        AccessPolicy::Translated {
            masks,
            flags,
            effect,
            fault,
        }
    }

    #[inline]
    fn resolve_ifetch_policy(csr: &mut CsrRegFile, with_side_effect: bool) -> AccessPolicy {
        let effect = if with_side_effect {
            AccessEffect::Accessed
        } else {
            AccessEffect::None
        };

        match csr.privelege_level() {
            PrivilegeLevel::M => AccessPolicy::Direct,
            PrivilegeLevel::S => AccessPolicy::Translated {
                masks: PTEFlags::X | PTEFlags::U,
                flags: PTEFlags::X,
                effect,
                fault: MemError::LoadPageFault,
            },
            PrivilegeLevel::U => AccessPolicy::Translated {
                masks: PTEFlags::X | PTEFlags::U,
                flags: PTEFlags::X | PTEFlags::U,
                effect,
                fault: MemError::LoadPageFault,
            },
            PrivilegeLevel::V => unreachable!(), // Doesn't have V-mode.
        }
    }

    fn translate_with_policy(
        &mut self,
        vaddr: WordType,
        policy: AccessPolicy,
    ) -> Result<u64, MemError> {
        match policy {
            AccessPolicy::Direct => Ok(vaddr),
            AccessPolicy::Translated {
                masks,
                flags,
                effect,
                fault,
            } => self
                .translate_vaddr(vaddr, masks, flags, effect)
                .map_err(|_| fault),
        }
    }

    pub(crate) fn read<T>(&mut self, addr: WordType, csr: &mut CsrRegFile) -> Result<T, MemError>
    where
        T: UnsignedInteger,
    {
        // Don't check alignment here since some devices may allow unaligned access.
        // Only check alignment in device's implementations.

        let policy = Self::resolve_data_policy(csr, AccessType::Read, true);
        let paddr = self.translate_with_policy(addr, policy)?;

        self.mmio.read_by_type(paddr)
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
        let policy = Self::resolve_data_policy(csr, AccessType::Write, true);
        let paddr = self.translate_with_policy(addr, policy)?;

        self.mmio.write_by_type(paddr, data)
    }

    pub(crate) fn load_reserved<T>(
        &mut self,
        addr: WordType,
        csr: &mut CsrRegFile,
    ) -> Result<T, MemError>
    where
        T: UnsignedInteger,
    {
        // TODO: Do we need to check alignment for lr/sc?
        if !crate::utils::check_align::<T>(addr) {
            return Err(MemError::LoadMisaligned);
        }

        let policy = Self::resolve_data_policy(csr, AccessType::Read, true);
        let paddr = self.translate_with_policy(addr, policy)?;

        self.mmio.load_reserved(paddr)
    }

    pub(crate) fn store_conditional<T>(
        &mut self,
        addr: WordType,
        data: T,
        csr: &mut CsrRegFile,
    ) -> Result<bool, MemError>
    where
        T: UnsignedInteger,
    {
        if !crate::utils::check_align::<T>(addr) {
            return Err(MemError::StoreMisaligned);
        }

        let policy = Self::resolve_data_policy(csr, AccessType::Write, true);
        let paddr = self.translate_with_policy(addr, policy)?;

        self.mmio.store_conditional(paddr, data)
    }

    pub(crate) fn ifetch<T>(&mut self, addr: WordType, csr: &mut CsrRegFile) -> Result<T, MemError>
    where
        T: UnsignedInteger,
    {
        let policy = Self::resolve_ifetch_policy(csr, true);
        let paddr = self.translate_with_policy(addr, policy)?;
        self.mmio.read_by_type(paddr)
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
        let policy = Self::resolve_ifetch_policy(csr, false);
        let paddr = self.translate_with_policy(addr, policy)?;
        self.mmio.read_by_type(paddr)
    }

    /// Atomic Memory Operation.
    pub(crate) fn fetch_and_op_amo<T, F>(
        &mut self,
        addr: WordType,
        rhs_val: T,
        csr: &mut CsrRegFile,
        f: F,
    ) -> Result<T, Exception>
    where
        T: UnsignedInteger,
        F: Fn(&T::AtomicType, T) -> Result<T, Exception>,
    {
        let policy = Self::resolve_data_policy(csr, AccessType::ReadWrite, true);
        let mut paddr = match self.translate_with_policy(addr, policy) {
            Ok(p) => p,
            Err(_) => return Err(Exception::StorePageFault),
        };

        if !crate::utils::check_align::<T>(paddr) {
            // FIXME: AMO instructions will do both load/store, which exception to raise?
            return Err(Exception::StoreMisaligned);
        }

        if !(ram_config::BASE_ADDR..ram_config::BASE_ADDR + ram_config::SIZE as WordType)
            .contains(&paddr)
        {
            // FIXME: Check manual to see what should we do here.
            return Err(Exception::StoreFault);
        }

        paddr -= ram_config::BASE_ADDR;

        let ram = unsafe { &mut *self.ram.get() };
        let ptr = &mut ram[paddr as usize] as *mut u8 as *mut T::AtomicType;
        let lhs = unsafe { &*ptr };

        f(lhs, rhs_val)
    }

    pub(crate) fn read_by_paddr<T>(&mut self, paddr: WordType) -> Result<T, MemError>
    where
        T: UnsignedInteger,
    {
        self.mmio.read_by_type(paddr.into())
    }

    pub(crate) fn write_by_paddr<T>(&mut self, paddr: WordType, data: T) -> Result<(), MemError>
    where
        T: UnsignedInteger,
    {
        self.mmio.write_by_type(paddr.into(), data)
    }

    #[cfg(test)]
    pub(crate) fn get_raw_ptr(&self) -> *mut u8 {
        unsafe { &mut *self.ram.get() }.get_raw_ptr()
    }

    // TODO: These debug functions (and their ability) are chaotic.
    // Think about them to determine what we really need.
    /// Read operation without side-effect of page table, provided for debugger.
    ///
    /// This function dones't respect the current privilege mode.
    pub(crate) fn debug_read<T>(&mut self, addr: Address) -> Result<T, MemError>
    where
        T: UnsignedInteger,
    {
        match addr {
            Address::Phys(addr) => self.read_by_paddr::<T>(addr),
            Address::Virt(addr) => {
                let (masks, flags) = (PTEFlags::empty(), PTEFlags::empty());

                if let Ok(paddr) = self.translate_vaddr(addr, masks, flags, AccessEffect::None) {
                    self.mmio.read_by_type(paddr)
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

                if let Ok(paddr) = self.translate_vaddr(addr, masks, flags, AccessEffect::None) {
                    self.mmio.write_by_type(paddr, data)
                } else {
                    Err(MemError::StorePageFault)
                }
            }
        }
    }

    fn translate_vaddr(
        &mut self,
        vaddr: WordType,
        masks: PTEFlags,
        target_flags: PTEFlags,
        effect: AccessEffect,
    ) -> Result<u64, PageTableError> {
        self.page_table
            .translate_vaddr(
                unsafe { self.ram.as_mut_unchecked() },
                vaddr.into(),
                masks,
                target_flags,
                effect,
            )
            .map(|addr| addr.into())
    }

    /// Translates a virtual address to a physical address without checking any PTE flags or updating any bits, provided for debugger.
    ///
    /// This function doesn't respect the current privilege mode or check PTE flags.
    pub(crate) fn debug_vaddr_to_paddr(&mut self, vaddr: WordType) -> Result<u64, PageTableError> {
        self.translate_vaddr(
            vaddr,
            PTEFlags::empty(),
            PTEFlags::empty(),
            AccessEffect::None,
        )
    }

    /// Translates an address to a physical address as a real instruction would, but without side effects (such as writing A/D bits).
    ///
    /// This means the function will check PTE flags, and consider CPU state like CSR settings and privilege mode.
    pub(crate) fn debug_translate(
        &mut self,
        addr: u64,
        access: AccessType,
        csr: &mut CsrRegFile,
    ) -> Result<u64, PageTableError> {
        let policy = Self::resolve_data_policy(csr, access, false);
        match policy {
            AccessPolicy::Direct => Ok(addr),
            AccessPolicy::Translated {
                masks,
                flags,
                effect,
                fault: _fault,
            } => self.translate_vaddr(addr, masks, flags, effect),
        }
    }

    // virtual memory
    pub fn set_mode(&mut self, mode: u8) {
        self.page_table.set_mode(mode);
    }

    pub fn set_root_ppn(&mut self, ppn: u64) {
        self.page_table.set_root_addr(ppn << PAGE_SIZE_XLEN);
    }

    pub fn set_ad_update_policy(&mut self, policy: AdUpdatePolicy) {
        self.page_table.set_ad_update_policy(policy);
    }

    // sync mmio devices
    pub fn sync(&mut self) {
        self.mmio.sync();
    }
}
