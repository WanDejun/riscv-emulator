use crate::config::arch_config::WordType;

pub const PAGE_SIZE_XLEN: usize = 12;
pub const PAGE_SIZE: WordType = 1 << PAGE_SIZE_XLEN;

pub const PHYSICAL_ADDR_WIDTH: usize = 56; // physical address width.
pub const PPN_WIDTH: usize = PHYSICAL_ADDR_WIDTH - PAGE_SIZE_XLEN; // PPN width size.
pub const PPN_MASK: WordType = ((1 << PPN_WIDTH) - 1) << 12;
pub const PPN_OFFSET: [WordType; 5] = [12, 21, 30, 39, 48];
pub const VPN_OFFSET: [WordType; 5] = [12, 21, 30, 39, 48];
pub const SUB_VPN_MASK: WordType = (1 << 9) - 1;
pub const PAGE_TABLE_LEVEL: [WordType; 3] = [3, 4, 5];

// ============================================
// ======= PTE flags in page table entry ======
// ============================================
pub const PTE_WIDTH_SIZE: usize = 10;
pub const PTE_FLAG_MASK: WordType = (1 << PTE_WIDTH_SIZE) - 1;
pub const PTE_PPN_MASK: WordType = ((1 << 44) - 1) << 10; // bits 10-53

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum VirtualMemoryMode {
    Page32bit = 0,
    Page39bit,
    Page48bit,
    Page57bit,
    // Page64bit,
    None = 0x3f,
}

#[inline]
pub fn get_page_table_level(mode: VirtualMemoryMode) -> usize {
    (mode as WordType + 2) as usize
}

// ============================================
// =================== SV39 ===================
// ============================================
#[cfg(test)]
pub const VIRTUAL_ADDR_WIDTH_SV39: usize = 39; // physical address width.
#[cfg(test)]
pub const VPN_WIDTH_SV39: usize = VIRTUAL_ADDR_WIDTH_SV39 - PAGE_SIZE_XLEN; // VPN width size.

// ============================================
// =================== SV48 ===================
// ============================================
#[cfg(test)]
pub const VIRTUAL_ADDR_WIDTH_SV48: usize = 48; // physical address width.
#[cfg(test)]
pub const VPN_WIDTH_SV48: usize = VIRTUAL_ADDR_WIDTH_SV48 - PAGE_SIZE_XLEN; // VPN width size.

// ============================================
// =================== SV57 ===================
// ============================================
#[cfg(test)]
pub const VIRTUAL_ADDR_WIDTH_SV57: usize = 55; // physical address width.
#[cfg(test)]
pub const VPN_WIDTH_SV57: usize = VIRTUAL_ADDR_WIDTH_SV57 - PAGE_SIZE_XLEN; // VPN width size.
