use bitflags::bitflags;

use crate::{
    config::arch_config::WordType,
    isa::riscv::{
        csr_reg::csr_macro::Satp,
        mmu::{
            address::{PageSize, PhysicalAddr, PhysicalPageNum, VirtualAddr, VirtualPageNum},
            config::{
                PAGE_SIZE_XLEN, PPN_MASK, PTE_FLAG_MASK, VirtualMemoryMode, get_page_table_level,
            },
        },
    },
    ram::Ram,
};

bitflags! {
    pub struct PTEFlags: u8 {
        const V = 1 << 0; // valid
        const R = 1 << 1; // read
        const W = 1 << 2; // write
        const X = 1 << 3; // execute
        const U = 1 << 4; // U-Mode
        const G = 1 << 5; // global mapping, will not be refreshed in TLB.
        const A = 1 << 6; // accessed, means this leaf-page has a mapping to physical memory.
        const D = 1 << 7; // dirty
    }
}

#[derive(Copy, Clone)]
#[repr(C)]
pub struct PageTableEntry {
    pub bits: WordType,
}

#[allow(unused)]
impl PageTableEntry {
    pub fn new(mem_value: WordType) -> Self {
        PageTableEntry { bits: mem_value }
    }

    pub fn empty() -> Self {
        PageTableEntry { bits: 0 }
    }

    pub fn ppn(&self) -> PhysicalPageNum {
        ((self.bits << 2) & PPN_MASK).into()
    }

    pub fn set_ppn(&mut self, ppn: PhysicalPageNum) {
        self.bits &= !(PPN_MASK >> 2);
        self.bits |= ppn.0 >> 2;
    }

    pub fn flags(&self) -> PTEFlags {
        PTEFlags::from_bits((self.bits & PTE_FLAG_MASK) as u8).unwrap()
    }

    pub fn is_page(&self) -> bool {
        self.bits & 0x0f == 0x01
    }

    // check flag in PTE.
    pub fn check_flag(&self, flag: PTEFlags) -> bool {
        !(self.flags() & flag).is_empty()
    }
    pub fn is_vaild(&self) -> bool {
        !(self.flags() & PTEFlags::V).is_empty()
    }
    pub fn is_readable(&self) -> bool {
        !(self.flags() & PTEFlags::R).is_empty()
    }
    pub fn is_writeable(&self) -> bool {
        !(self.flags() & PTEFlags::W).is_empty()
    }
    pub fn is_executeble(&self) -> bool {
        !(self.flags() & PTEFlags::X).is_empty()
    }
    pub fn is_u_mode(&self) -> bool {
        !(self.flags() & PTEFlags::U).is_empty()
    }
    pub fn is_global(&self) -> bool {
        !(self.flags() & PTEFlags::G).is_empty()
    }
    pub fn is_accessed(&self) -> bool {
        !(self.flags() & PTEFlags::A).is_empty()
    }
    pub fn is_dirty(&self) -> bool {
        !(self.flags() & PTEFlags::G).is_empty()
    }

    // set flag for PTE.
    pub fn set_flag(&mut self, flag: PTEFlags) {
        self.bits |= flag.bits() as WordType;
    }
    pub fn set_vaild(&mut self) {
        self.bits |= PTEFlags::V.bits() as WordType;
    }
    pub fn set_readable(&mut self) {
        self.bits |= PTEFlags::R.bits() as WordType;
    }
    pub fn set_writeable(&mut self) {
        self.bits |= PTEFlags::W.bits() as WordType;
    }
    pub fn set_executeble(&mut self) {
        self.bits |= PTEFlags::X.bits() as WordType;
    }
    pub fn set_u_mode(&mut self) {
        self.bits |= PTEFlags::U.bits() as WordType;
    }
    pub fn set_global(&mut self) {
        self.bits |= PTEFlags::G.bits() as WordType;
    }
    pub fn set_accessed(&mut self) {
        self.bits |= PTEFlags::A.bits() as WordType;
    }
    pub fn set_dirty(&mut self) {
        self.bits |= PTEFlags::G.bits() as WordType;
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PageTableError {
    AlignFault,
    PageFault,
    PrivilegeFault,
}

pub struct PageTable {
    root_ppn: PhysicalPageNum,
    mode: VirtualMemoryMode,
    // TODO: Add TLB here.
}

impl PageTable {
    pub fn new(root_ppn: PhysicalPageNum, mode: VirtualMemoryMode) -> Self {
        Self { root_ppn, mode }
    }

    pub fn updata(&mut self, satp: Satp) {
        self.mode = match satp.get_mode() {
            8 => VirtualMemoryMode::Page32bit,
            9 => VirtualMemoryMode::Page39bit,
            10 => VirtualMemoryMode::Page48bit,
            11 => VirtualMemoryMode::Page57bit,
            _ => VirtualMemoryMode::None,
        }
    }

    // TODO: Maybe we need to take shared owership in virtual memory manager to avoid intermediate overhead
    // TODO: Read/Write/Execute check.
    // TODO: Privilege check.
    pub fn translate_addr(
        &self,
        mem: &mut Ram,
        vaddr: VirtualAddr,
        flag: PTEFlags,
    ) -> Result<PhysicalAddr, PageTableError> {
        if self.mode == VirtualMemoryMode::None {
            return Ok(vaddr.0.into());
        }

        let target_pte = self.find_pte(mem, vaddr.floor())?;
        if !target_pte.0.check_flag(flag) {
            return Err(PageTableError::PrivilegeFault);
        }
        let ppn = target_pte.0.ppn();
        let paddr = match target_pte.1 {
            PageSize::Small4K => ppn.0 | (vaddr.0 & ((1 << (1 * PAGE_SIZE_XLEN as WordType)) - 1)),
            PageSize::Medium2M => ppn.0 | (vaddr.0 & ((1 << (2 * PAGE_SIZE_XLEN as WordType)) - 1)),
            PageSize::Large1G => ppn.0 | (vaddr.0 & ((1 << (3 * PAGE_SIZE_XLEN as WordType)) - 1)),
        };
        Ok(paddr.into())
    }

    fn find_pte(
        &self,
        mem: &mut Ram,
        vpn: VirtualPageNum,
    ) -> Result<(&mut PageTableEntry, PageSize), PageTableError> {
        let level = get_page_table_level(self.mode);
        let mut entry = self.root_ppn;
        let sub_vpn_array = vpn.get_sub_vpn();

        for i in (0..level).rev() {
            let sub_vpn = sub_vpn_array[i];
            let pte = &mut entry.get_pte_array(mem)[sub_vpn as usize];
            if !pte.is_vaild() {
                return Err(PageTableError::PageFault);
            }

            if i == 0 || !pte.is_page() {
                return Ok((pte, PageSize::from(i as u8)));
            }

            entry = pte.ppn();
        }

        unreachable!()
    }
}

#[cfg(test)]
mod test {
    use crate::ram_config;

    use super::*;

    #[test]
    fn page_table_test() {
        let mut ram: Ram = Ram::new();
        let ppn0 = 0x8000_1000u64;
        let ppn1 = 0x8000_2000u64;
        let ppn2 = 0x8000_3000u64;
        let data_page = 0x8000_4000u64;

        let mut pte = PageTableEntry::empty();
        pte.set_vaild();

        // level 0
        pte.set_ppn(ppn1.into());
        ram.write(ppn0 - ram_config::BASE_ADDR, pte.bits).unwrap();

        // level 1
        pte.set_ppn(ppn2.into());
        ram.write(ppn1 - ram_config::BASE_ADDR, pte.bits).unwrap();

        // level 2
        pte.set_ppn(data_page.into());
        pte.set_readable();
        pte.set_writeable();
        ram.write(ppn2 - ram_config::BASE_ADDR, pte.bits).unwrap();

        let page_table = PageTable::new(ppn0.into(), VirtualMemoryMode::Page39bit);
        let data_pte = page_table.find_pte(&mut ram, 0x0000_0010.into()).unwrap();
        assert_eq!(pte.bits, data_pte.0.bits);

        let paddr = page_table
            .translate_addr(&mut ram, 0x0000_0123.into(), PTEFlags::R)
            .unwrap();
        assert_eq!(paddr.0, data_page | 0x123);
    }

    #[test]
    fn big_page_test() {
        // 2MB Page.
        let mut ram: Ram = Ram::new();
        let ppn0 = 0x8000_1000u64;
        let ppn1 = 0x8000_2000u64;
        let data_page = 0x8100_0000u64;

        let mut pte = PageTableEntry::empty();
        pte.set_vaild();

        // level 0
        pte.set_ppn(ppn1.into());
        ram.write(ppn0 - ram_config::BASE_ADDR, pte.bits).unwrap();

        // level 1
        pte.set_ppn(data_page.into());
        pte.set_readable();
        pte.set_writeable();
        ram.write(ppn1 - ram_config::BASE_ADDR, pte.bits).unwrap();

        let page_table = PageTable::new(ppn0.into(), VirtualMemoryMode::Page39bit);
        let data_pte = page_table.find_pte(&mut ram, 0x0000_8000.into()).unwrap();
        assert_eq!(pte.bits, data_pte.0.bits);

        let paddr = page_table
            .translate_addr(&mut ram, 0x0011_4514.into(), PTEFlags::W)
            .unwrap();
        assert_eq!(paddr.0, 0x8111_4514);
    }

    #[test]
    fn rwx_authority_test() {
        let mut ram: Ram = Ram::new();
        let ppn0 = 0x8000_1000u64;
        let ppn1 = 0x8000_2000u64;
        let ppn2 = 0x8000_3000u64;
        let data_page = 0x8000_4000u64;

        let mut pte = PageTableEntry::empty();
        pte.set_vaild();

        // level 0
        pte.set_ppn(ppn1.into());
        ram.write(ppn0 - ram_config::BASE_ADDR, pte.bits).unwrap();

        // level 1
        pte.set_ppn(ppn2.into());
        ram.write(ppn1 - ram_config::BASE_ADDR, pte.bits).unwrap();

        // level 2
        pte.set_ppn(data_page.into());
        pte.set_readable();
        pte.set_writeable();
        ram.write(ppn2 - ram_config::BASE_ADDR, pte.bits).unwrap();

        let page_table = PageTable::new(ppn0.into(), VirtualMemoryMode::Page39bit);

        // try get instr, without X authority.
        let err = page_table
            .translate_addr(&mut ram, 0x0000_0010.into(), PTEFlags::X)
            .unwrap_err();
        assert_eq!(err, PageTableError::PrivilegeFault);
    }
}
