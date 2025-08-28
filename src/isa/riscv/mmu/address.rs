use std::slice;

use crate::{
    config::arch_config::WordType,
    isa::riscv::mmu::{
        config::{
            PAGE_SIZE, PAGE_SIZE_XLEN, PHYSICAL_ADDR_WIDTH, PPN_WIDTH, SUB_VPN_MASK, VPN_OFFSET,
            VPN_WIDTH_SV57,
        },
        page_table::PageTableEntry,
    },
    ram::Ram,
    ram_config,
};

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct PhysicalAddr(pub u64); // rv32 physical addr is 34bit-length.

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct VirtualAddr(pub WordType);

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct PhysicalPageNum(pub u64); // PPN

#[derive(Copy, Clone, Ord, PartialOrd, Eq, PartialEq, Debug)]
pub struct VirtualPageNum(pub WordType); // VPN

#[repr(u8)]
#[derive(Debug, PartialEq, Eq)]
pub enum PageSize {
    Small4K = 0,
    Medium2M,
    Large1G,
}

impl From<u8> for PageSize {
    fn from(value: u8) -> Self {
        unsafe { std::mem::transmute(value) }
    }
}

#[rustfmt::skip]
impl PhysicalAddr {
    pub fn get_offset(self, page_size: PageSize) -> WordType {
        match page_size {
            PageSize::Small4K  => self.0 & ((1 << (1 * PAGE_SIZE_XLEN)) - 1),
            PageSize::Medium2M => self.0 & ((1 << (2 * PAGE_SIZE_XLEN)) - 1),
            PageSize::Large1G  => self.0 & ((1 << (3 * PAGE_SIZE_XLEN)) - 1),
        }
    }
    pub fn ceil(self) -> PhysicalPageNum {
        PhysicalPageNum(self.0 >> PAGE_SIZE_XLEN)
    }
    pub fn floor(self) -> PhysicalPageNum {
        PhysicalPageNum((self.0 + (PAGE_SIZE - 1)) >> PAGE_SIZE_XLEN)
    }
    pub fn is_aligned(self) -> bool {
        self.get_offset(PageSize::Small4K) == 0
    }
}
impl VirtualAddr {
    pub fn get_offset(self, page_size: PageSize) -> WordType {
        match page_size {
            PageSize::Small4K => self.0 & ((1 << (1 * PAGE_SIZE_XLEN)) - 1),
            PageSize::Medium2M => self.0 & ((1 << (2 * PAGE_SIZE_XLEN)) - 1),
            PageSize::Large1G => self.0 & ((1 << (3 * PAGE_SIZE_XLEN)) - 1),
        }
    }
    pub fn ceil(self) -> VirtualPageNum {
        VirtualPageNum((self.0 + (PAGE_SIZE - 1)) & !(PAGE_SIZE - 1))
    }
    pub fn floor(self) -> VirtualPageNum {
        VirtualPageNum(self.0 & !(PAGE_SIZE - 1))
    }
    pub fn is_aligned(self) -> bool {
        self.get_offset(PageSize::Small4K) == 0
    }
}

impl PhysicalPageNum {
    pub fn get_byte_array(&self, mem: &mut Ram) -> &'static mut [u8] {
        let ptr = &mut mem[(self.0 - ram_config::BASE_ADDR) as usize] as *mut u8;
        unsafe { slice::from_raw_parts_mut(ptr, PAGE_SIZE as usize) }
    }
    pub fn get_pte_array(&self, mem: &mut Ram) -> &'static mut [PageTableEntry] {
        let ptr = &mut mem[(self.0 - ram_config::BASE_ADDR) as usize] as *mut u8 as usize;
        #[cfg(feature = "riscv64")]
        unsafe {
            slice::from_raw_parts_mut(ptr as *mut PageTableEntry, 512)
        }
        #[cfg(feature = "riscv32")]
        unsafe {
            slice::from_raw_parts_mut(ptr as *mut PageTableEntry, 1024)
        }
    }
    pub fn get_mut<T>(&self, mem: &mut Ram) -> &'static mut T {
        let ptr = &mut mem[(self.0 - ram_config::BASE_ADDR) as usize] as *mut u8 as usize;
        unsafe { (ptr as *mut T).as_mut().unwrap() }
    }
    pub fn step(&mut self) {
        self.0 += 1;
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

// PPN
impl From<WordType> for PhysicalPageNum {
    fn from(value: WordType) -> Self {
        PhysicalPageNum(value & ((1 << PPN_WIDTH) - 1))
    }
}
impl From<PhysicalPageNum> for WordType {
    fn from(ppn: PhysicalPageNum) -> Self {
        ppn.0
    }
}

// Cast between PPN and physical address.
impl From<PhysicalPageNum> for PhysicalAddr {
    fn from(ppn: PhysicalPageNum) -> Self {
        PhysicalAddr(ppn.0)
    }
}
impl From<PhysicalAddr> for PhysicalPageNum {
    fn from(addr: PhysicalAddr) -> Self {
        // debug_assert_eq!(addr.get_offset(), 0);
        PhysicalPageNum(addr.0)
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

// VPN
impl From<WordType> for VirtualPageNum {
    fn from(value: WordType) -> Self {
        VirtualPageNum(value & ((1 << VPN_WIDTH_SV57) - 1))
    }
}
impl From<VirtualPageNum> for WordType {
    fn from(vpn: VirtualPageNum) -> Self {
        vpn.0
    }
}

// Cast between VPN and virtual address.
impl From<VirtualPageNum> for VirtualAddr {
    fn from(vpn: VirtualPageNum) -> Self {
        VirtualAddr(vpn.0)
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
    pub fn get_sub_vpn(&self) -> [WordType; 5] {
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
        (self.0 >> VPN_OFFSET[INDEX]) & SUB_VPN_MASK
    }
}
