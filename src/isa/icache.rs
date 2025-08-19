use crate::{
    config::arch_config::WordType,
    isa::{ISATypes, riscv::RiscvTypes},
};

pub(super) trait ICache<I: ISATypes> {
    fn new() -> Self;
    fn get(&self, addr: WordType) -> Option<I::DecodeRst>;
    fn put(&mut self, addr: WordType, data: I::DecodeRst);
    fn invalidate(&mut self, addr: WordType);
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

impl<I: ISATypes + ToGroupId, const N: usize> ICache<I> for DirectICache<I, N> {
    fn new() -> Self {
        debug_assert!(N > 0 && (N & (N - 1)) == 0, "N must be a power of two");
        Self {
            cache: [(0, None); N],
        }
    }

    #[inline]
    fn get(&self, addr: WordType) -> Option<I::DecodeRst> {
        let (tag, data) = &self.cache[Self::get_group_id(addr)];
        if *tag == addr { data.clone() } else { None }
    }

    #[inline]
    fn put(&mut self, addr: WordType, data: I::DecodeRst) {
        self.cache[Self::get_group_id(addr)] = (addr, Some(data));
    }

    #[inline]
    fn invalidate(&mut self, addr: WordType) {
        self.cache[Self::get_group_id(addr)] = (0, None);
    }
}

// Set-associative iCache

struct SetICacheLine<I: ISATypes, const S: usize> {
    nxt_idx: usize,
    source_addr: [WordType; S],
    data: [Option<I::DecodeRst>; S],
}

impl<I: ISATypes, const S: usize> SetICacheLine<I, S> {
    const fn new() -> Self {
        Self {
            nxt_idx: 0,
            source_addr: [0; S],
            data: [None; S],
        }
    }

    #[inline]
    fn insert(&mut self, addr: WordType, data: I::DecodeRst) {
        self.source_addr[self.nxt_idx] = addr;
        self.data[self.nxt_idx] = Some(data);

        self.nxt_idx += 1;
        if self.nxt_idx == S {
            self.nxt_idx = 0;
        }
    }

    fn invalidate(&mut self, addr: WordType) {
        if let Some(index) = self.source_addr.iter().position(|&item| item == addr) {
            self.source_addr[index] = 0;
            self.data[index] = None;
        }
    }
}

pub(super) struct SetICache<I: ISATypes, const N: usize, const S: usize> {
    cache: [SetICacheLine<I, S>; N],
}

impl<I: ISATypes + ToGroupId, const N: usize, const S: usize> SetICache<I, N, S> {
    const SLEN: u32 = S.trailing_zeros();

    #[inline]
    fn get_group_id(addr: WordType) -> usize {
        (I::group_id(addr).wrapping_shr(Self::SLEN)) & (N - 1)
    }
}

impl<I: ISATypes + ToGroupId, const N: usize, const S: usize> ICache<I> for SetICache<I, N, S> {
    fn new() -> Self {
        debug_assert!(N > 0 && (N & (N - 1)) == 0, "N must be a power of two.");
        debug_assert!(S > 0 && (S & (S - 1)) == 0, "S must be a power of two.");

        Self {
            cache: std::array::from_fn(|_| SetICacheLine::new()),
        }
    }

    #[inline]
    fn get(&self, addr: WordType) -> Option<I::DecodeRst> {
        let group_id = Self::get_group_id(addr);
        let line = &self.cache[group_id];

        line.source_addr
            .iter()
            .position(|&item| item == addr)
            .and_then(|index| line.data[index].clone())
    }

    #[inline]
    fn put(&mut self, addr: WordType, data: I::DecodeRst) {
        self.cache[Self::get_group_id(addr)].insert(addr, data);
    }

    #[inline]
    fn invalidate(&mut self, addr: WordType) {
        self.cache[Self::get_group_id(addr)].invalidate(addr);
    }
}
