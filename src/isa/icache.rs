use crate::{
    config::arch_config::WordType,
    isa::{ISATypes, riscv::RiscvTypes},
};

pub(super) trait ICache<I: ISATypes> {
    fn new() -> Self;
    fn get(&self, addr: WordType) -> Option<I::DecodeRst>;
    fn put(&mut self, addr: WordType, data: I::DecodeRst);
}

pub(super) trait ToGroupId {
    fn group_id(addr: WordType) -> usize;
}

impl ToGroupId for RiscvTypes {
    #[inline]
    fn group_id(addr: WordType) -> usize {
        (addr as usize) >> 1
    }
}

/// Direct-mapped iCache.
pub(super) struct DirectICache<I: ISATypes, const N: usize> {
    cache: [(WordType, Option<I::DecodeRst>); N],
}

impl<I: ISATypes + ToGroupId, const N: usize> DirectICache<I, N> {
    #[inline]
    fn get_group_id(addr: WordType) -> usize {
        I::group_id(addr) & (N - 1)
    }
}

impl<I: ISATypes, const N: usize> ICache<I> for DirectICache<I, N> {
    fn new() -> Self {
        debug_assert!(N > 0 && (N & (N - 1)) == 0, "N must be a power of two");
        Self {
            cache: [(0, None); N],
        }
    }

    #[inline]
    fn get(&self, addr: WordType) -> Option<I::DecodeRst> {
        let (tag, data) = &self.cache[addr as usize & (N - 1)];
        if *tag == addr { data.clone() } else { None }
    }

    #[inline]
    fn put(&mut self, addr: WordType, data: I::DecodeRst) {
        self.cache[addr as usize & (N - 1)] = (addr, Some(data));
    }
}
