use std::slice;

use crate::{
    config::arch_config::WordType,
    isa::riscv::mmu::{
        config::{PAGE_SIZE, PAGE_SIZE_XLEN, PHYSICAL_ADDR_WIDTH, SUB_VPN_MASK, VPN_OFFSET},
        page_table::PageTableEntry,
    },
    ram::Ram,
    ram_config,
};

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub(super) struct PhysicalAddr(pub(super) u64); // rv32 physical addr is 34bit-length.

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub(super) struct VirtualAddr(pub(super) WordType);

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub(super) struct PhysicalPageNum {
    pub(super) address: u64,
}

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub(super) struct VirtualPageNum {
    pub(super) address: WordType,
}

#[repr(u8)]
#[derive(Debug, PartialEq, Eq)]
pub(super) enum PageSize {
    Small4K = 0,
    Medium2M,
    Large1G,
}

impl From<u8> for PageSize {
    fn from(value: u8) -> Self {
        match value {
            0 => PageSize::Small4K,
            1 => PageSize::Medium2M,
            2 => PageSize::Large1G,
            _ => panic!("Invalid page size value: {}", value),
        }
    }
}

impl PhysicalAddr {
    #[rustfmt::skip]
    pub(super) fn get_offset(self, page_size: PageSize) -> WordType {
        match page_size {
            PageSize::Small4K  => self.0 & ((1 << (1 * PAGE_SIZE_XLEN)) - 1),
            PageSize::Medium2M => self.0 & ((1 << (2 * PAGE_SIZE_XLEN)) - 1),
            PageSize::Large1G  => self.0 & ((1 << (3 * PAGE_SIZE_XLEN)) - 1),
        }
    }
    pub(super) fn ceil(self) -> PhysicalPageNum {
        PhysicalPageNum::from_paddr(self.0)
    }
    pub(super) fn floor(self) -> PhysicalPageNum {
        PhysicalPageNum::from_paddr((self.0 + (PAGE_SIZE - 1)) & !(PAGE_SIZE - 1))
    }
    pub(super) fn is_aligned(self) -> bool {
        self.get_offset(PageSize::Small4K) == 0
    }
}
impl VirtualAddr {
    pub(super) fn get_offset(self, page_size: PageSize) -> WordType {
        match page_size {
            PageSize::Small4K => self.0 & ((1 << (1 * PAGE_SIZE_XLEN)) - 1),
            PageSize::Medium2M => self.0 & ((1 << (2 * PAGE_SIZE_XLEN)) - 1),
            PageSize::Large1G => self.0 & ((1 << (3 * PAGE_SIZE_XLEN)) - 1),
        }
    }
    pub(super) fn ceil(self) -> VirtualPageNum {
        VirtualPageNum::from_vaddr((self.0 + (PAGE_SIZE - 1)) & !(PAGE_SIZE - 1))
    }
    pub(super) fn floor(self) -> VirtualPageNum {
        VirtualPageNum::from_vaddr(self.0 & !(PAGE_SIZE - 1))
    }
    pub(super) fn is_aligned(self) -> bool {
        self.get_offset(PageSize::Small4K) == 0
    }
}

impl PhysicalPageNum {
    pub(super) fn get_byte_array(&self, mem: &mut Ram) -> &'static mut [u8] {
        let ptr = &mut mem[(self.address - ram_config::BASE_ADDR) as usize] as *mut u8;
        unsafe { slice::from_raw_parts_mut(ptr, PAGE_SIZE as usize) }
    }
    pub(super) fn get_pte_array(&self, mem: &mut Ram) -> &'static mut [PageTableEntry] {
        let ptr = &mut mem[(self.address - ram_config::BASE_ADDR) as usize] as *mut u8 as usize;
        #[cfg(feature = "riscv64")]
        unsafe {
            slice::from_raw_parts_mut(ptr as *mut PageTableEntry, 512)
        }
        #[cfg(feature = "riscv32")]
        unsafe {
            slice::from_raw_parts_mut(ptr as *mut PageTableEntry, 1024)
        }
    }
    pub(super) fn from_ppn(ppn: WordType) -> Self {
        PhysicalPageNum {
            address: ppn << PAGE_SIZE_XLEN,
        }
    }
    pub(super) fn from_paddr(paddr: WordType) -> Self {
        PhysicalPageNum {
            address: paddr & !(PAGE_SIZE - 1),
        }
    }
}

// PhysicalAddr
impl From<u64> for PhysicalAddr {
    fn from(value: u64) -> Self {
        PhysicalAddr(value & ((1 << PHYSICAL_ADDR_WIDTH) - 1))
    }
}
impl From<PhysicalAddr> for u64 {
    fn from(addr: PhysicalAddr) -> Self {
        addr.0
    }
}

// Cast between PPN and physical address.
impl From<PhysicalPageNum> for PhysicalAddr {
    fn from(ppn: PhysicalPageNum) -> Self {
        PhysicalAddr(ppn.address)
    }
}
impl From<PhysicalAddr> for PhysicalPageNum {
    fn from(addr: PhysicalAddr) -> Self {
        // debug_assert_eq!(addr.get_offset(), 0);
        PhysicalPageNum::from_paddr(addr.0)
    }
}

// VirtualAddr
impl From<WordType> for VirtualAddr {
    fn from(value: WordType) -> Self {
        VirtualAddr(value & ((1 << PHYSICAL_ADDR_WIDTH) - 1))
    }
}
impl From<VirtualAddr> for WordType {
    fn from(addr: VirtualAddr) -> Self {
        addr.0
    }
}

// Cast between VPN and virtual address.
impl From<VirtualPageNum> for VirtualAddr {
    fn from(vpn: VirtualPageNum) -> Self {
        VirtualAddr(vpn.address)
    }
}
impl From<VirtualAddr> for VirtualPageNum {
    fn from(addr: VirtualAddr) -> Self {
        // debug_assert_eq!(addr.get_offset(), 0);
        addr.floor()
    }
}

// get sub_vpn.
impl VirtualPageNum {
    pub(super) fn from_vpn(vpn: WordType) -> Self {
        VirtualPageNum {
            address: vpn << PAGE_SIZE_XLEN,
        }
    }
    pub(super) fn from_vaddr(vaddr: WordType) -> Self {
        VirtualPageNum {
            address: vaddr & !(PAGE_SIZE - 1),
        }
    }

    pub(super) fn get_sub_vpn(&self) -> [WordType; 5] {
        [
            self.get_vpn::<0>(),
            self.get_vpn::<1>(),
            self.get_vpn::<2>(),
            self.get_vpn::<3>(),
            self.get_vpn::<4>(),
        ]
    }

    #[inline]
    fn get_vpn<const INDEX: usize>(&self) -> WordType {
        (self.address >> VPN_OFFSET[INDEX]) & SUB_VPN_MASK
    }
}
