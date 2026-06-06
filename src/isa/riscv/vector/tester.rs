#![cfg(test)]

use std::{cell::UnsafeCell, fmt::Debug, rc::Rc};

use crate::{
    config::arch_config::WordType, device::mmio::MemoryMapIO, ram::Ram, utils::UnsignedInteger,
};

use super::{
    Vector,
    types::{Vlmul, Vsew},
};

pub(super) struct VectorBuilder {
    vector: Vector,
    mmio: MemoryMapIO,
}

impl VectorBuilder {
    pub(super) fn new() -> Self {
        let ram = Rc::new(UnsafeCell::new(Ram::new()));
        let mmio = MemoryMapIO::from_mmio_items(ram, vec![]);
        Self {
            vector: Vector::new(),
            mmio,
        }
    }

    pub(super) fn config(
        mut self,
        vlmul: Vlmul,
        vsew: Vsew,
        mask_agnostic: bool,
        tail_agnostic: bool,
        vl: u16,
    ) -> Self {
        self.vector
            .set_config((vlmul, vsew, tail_agnostic, mask_agnostic, vl));
        self
    }

    pub(super) fn reg<T>(mut self, lmul: u8, idx: u8, value: &[T]) -> Self {
        self.vector.write_as_type(lmul, idx, value);
        self
    }

    pub(super) fn mem<T>(mut self, addr: WordType, value: T) -> Self
    where
        T: UnsignedInteger,
    {
        self.mmio.write_by_type(addr, value).unwrap();
        self
    }

    pub(super) fn mem_range<It, F, T>(mut self, indexes: It, mut f: F) -> Self
    where
        It: Iterator<Item = usize>,
        F: FnMut(usize) -> (WordType, T),
        T: UnsignedInteger,
    {
        for i in indexes {
            let (addr, value) = f(i);
            self.mmio.write_by_type(addr, value).unwrap();
        }
        self
    }

    pub(super) fn build(self) -> (Vector, MemoryMapIO) {
        (self.vector, self.mmio)
    }
}

pub(super) struct VectorChecker<'a> {
    pub(super) vector: &'a mut Vector,
    pub(super) mmio: &'a mut MemoryMapIO,
}

impl<'a> VectorChecker<'a> {
    pub(super) fn new(vector: &'a mut Vector, mmio: &'a mut MemoryMapIO) -> Self {
        Self { vector, mmio }
    }

    pub(super) fn reg<T>(self, lmul: u8, idx: u8, value: &[T]) -> Self
    where
        T: Eq + Debug,
    {
        let reg_val = self
            .vector
            .vector_regfile
            .read_as_type::<T>(lmul, idx)
            .unwrap();
        assert_eq!(reg_val.len(), value.len());
        for i in 0..reg_val.len() {
            assert_eq!(
                reg_val[i], value[i],
                "Vector register #{idx} [{i}] incorrect"
            );
        }
        self
    }

    pub(super) fn mem<T>(self, addr: WordType, value: T) -> Self
    where
        T: UnsignedInteger + Debug,
    {
        let got = self.mmio.read_by_type::<T>(addr).unwrap();
        assert_eq!(got, value, "Memory value incorrect at pos {}", addr);
        self
    }

    pub(super) fn customized<F>(self, f: F) -> Self
    where
        F: FnOnce(Self) -> Self,
    {
        f(self)
    }
}
