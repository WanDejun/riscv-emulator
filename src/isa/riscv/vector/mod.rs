use crate::{
    config::arch_config::WordType,
    cpu::VectorRegFile,
    device::{DeviceTrait, mmio::MemoryMapIO},
    isa::riscv::{
        trap::Exception,
        vector::types::{VGFRef, VGFRefMut, VectorConfig, Vlmul, Vsew},
    },
};

pub mod integer;
pub mod types;
pub const VLEN: usize = 128;
pub const VLEN_BYTE: usize = VLEN >> 3;

pub(super) struct Vector {
    config: VectorConfig,
    vector_regfile: VectorRegFile,
}

pub(super) trait VectorGetAddrTrait {
    fn exec(&self, base: WordType, index: WordType) -> WordType;
}

pub(super) struct VectorUnitMemLoad {
    vsew: Vsew,
}

impl VectorGetAddrTrait for VectorUnitMemLoad {
    #[inline(always)]
    fn exec(&self, base: WordType, index: WordType) -> WordType {
        base + (index << self.vsew as u8)
    }
}

impl Vector {
    pub(super) fn new() -> Self {
        Self {
            config: VectorConfig::new(),
            vector_regfile: VectorRegFile::new(),
        }
    }

    #[inline(always)]
    pub(super) fn set_config(&mut self, vlmul_vsew_ta_ma_vl: (Vlmul, Vsew, bool, bool, u16)) {
        (
            self.config.vlmul,
            self.config.vsew,
            self.config.tail_agnostic,
            self.config.mask_agnostic,
            self.config.vl,
        ) = vlmul_vsew_ta_ma_vl;
    }

    #[inline]
    pub(super) fn read_as_type<T>(&self, idx: u8) -> Result<&[T], Exception> {
        self.vector_regfile
            .read_as_type(self.config.vlmul.get_lmul(), idx)
    }

    #[inline]
    pub(super) fn write_as_type<T>(&mut self, lmul: u8, idx: u8, value: &[T]) {
        self.vector_regfile.write(lmul, idx, value, 1).unwrap();
    }

    pub(super) fn unit_stride_load(
        &mut self,
        vd: u8,
        eew: Vsew,
        seg: u8,
        base_addr: WordType,
        mem: &mut MemoryMapIO,
    ) -> Result<(), Exception> {
        let f = VectorUnitMemLoad { vsew: eew };
        let lmul = self.config.vlmul.get_lmul();
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut(lmul, vd, seg)?,
            eew.get_sew(),
            lmul,
            seg,
        );
        let mut err = Ok(());
        vd_ref
            .iter_mut()
            .enumerate()
            .filter(|v| v.0 < self.config.vl as usize)
            .for_each(|(index, element)| match eew {
                Vsew::E8 => match mem.read_u8(f.exec(base_addr, index as WordType)) {
                    Ok(ram_value) => element.set(ram_value),
                    Err(e) => err = Err(e),
                },
                Vsew::E16 => match mem.read_u16(f.exec(base_addr, index as WordType)) {
                    Ok(ram_value) => element.set(ram_value),
                    Err(e) => err = Err(e),
                },
                Vsew::E32 => match mem.read_u32(f.exec(base_addr, index as WordType)) {
                    Ok(ram_value) => element.set(ram_value),
                    Err(e) => err = Err(e),
                },
                Vsew::E64 => match mem.read_u64(f.exec(base_addr, index as WordType)) {
                    Ok(ram_value) => element.set(ram_value),
                    Err(e) => err = Err(e),
                },
            });
        match err {
            Err(err) => Err(err.into()),
            Ok(()) => Ok(()),
        }
    }

    pub(super) fn unit_stride_store(
        &mut self,
        vs: u8,
        eew: Vsew,
        seg: u8,
        base_addr: WordType,
        mem: &mut MemoryMapIO,
    ) -> Result<(), Exception> {
        let f = VectorUnitMemLoad { vsew: eew };
        let lmul = self.config.vlmul.get_lmul();
        let vd_ref = VGFRef::new(
            self.vector_regfile.read(lmul, vs)?,
            eew.get_sew(),
            lmul,
            seg,
        );
        let mut err = Ok(());
        vd_ref
            .iter()
            .enumerate()
            .filter(|v| v.0 < self.config.vl as usize)
            .for_each(|(index, element)| match eew {
                Vsew::E8 => mem
                    .write_u8(f.exec(base_addr, index as WordType), element.get())
                    .unwrap_or_else(|e| err = Err(e.into())),
                Vsew::E16 => mem
                    .write_u16(f.exec(base_addr, index as WordType), element.get())
                    .unwrap_or_else(|e| err = Err(e.into())),
                Vsew::E32 => mem
                    .write_u32(f.exec(base_addr, index as WordType), element.get())
                    .unwrap_or_else(|e| err = Err(e.into())),
                Vsew::E64 => mem
                    .write_u64(f.exec(base_addr, index as WordType), element.get())
                    .unwrap_or_else(|e| err = Err(e.into())),
            });
        err
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{ram::Ram, ram_config::BASE_ADDR};
    use std::{cell::UnsafeCell, rc::Rc};

    #[test]
    fn test_unit_stride_load() {
        let mut vector_regfile = Vector::new();
        let mut ram = Ram::new();
        for i in 0..128 {
            ram.write(i, 1 + (i as u8 * 2)).unwrap();
        }
        let mut mmio = MemoryMapIO::from_mmio_items(Rc::new(UnsafeCell::new(ram)), vec![]);

        // --------------- Seg = 1 ---------------
        // write as m2, e8
        vector_regfile.config.vlmul = Vlmul::M2;
        vector_regfile.config.vl = VLEN_BYTE as u16 * Vlmul::M2.get_lmul() as u16;
        vector_regfile
            .unit_stride_load(2, Vsew::E8, 1, BASE_ADDR, &mut mmio)
            .unwrap();
        // read as m1, e8
        let vector_ref = vector_regfile
            .vector_regfile
            .read_as_type::<u8>(Vlmul::M1.get_lmul(), 3)
            .unwrap();
        // println!("{:?}", vector_ref);
        assert_eq!(vector_ref[2], (1 + VLEN_BYTE * 2) as u8 + 2 * 2);

        // --------------- Seg = 2 ---------------
        // write as m1, e8
        vector_regfile.config.vlmul = Vlmul::M1;
        vector_regfile.config.vl = VLEN_BYTE as u16 * Vlmul::M2.get_lmul() as u16;
        vector_regfile
            .unit_stride_load(2, Vsew::E8, 2, BASE_ADDR, &mut mmio)
            .unwrap();
        // read as m1, e8
        let vector_ref = vector_regfile
            .vector_regfile
            .read_as_type::<u8>(Vlmul::M1.get_lmul(), 3)
            .unwrap();
        // println!("{:?}", vector_ref);
        assert_eq!(vector_ref[2], 3 + 2 * 4);
    }

    #[test]
    fn test_unit_stride_store() {
        let mut vector_regfile = Vector::new();
        let ram = Ram::new();
        let mut mmio = MemoryMapIO::from_mmio_items(Rc::new(UnsafeCell::new(ram)), vec![]);

        let store_addr = BASE_ADDR + 0x1000;
        // --------------- M2, E8 ---------------
        vector_regfile.config.vlmul = Vlmul::M2;
        vector_regfile.config.vl = VLEN_BYTE as u16 * Vlmul::M2.get_lmul() as u16;
        // write known test pattern into v2 (M2 group: v2, v3)
        let test_values: Vec<u8> = (0..VLEN_BYTE * 2).map(|i| (i * 3 + 1) as u8).collect();
        vector_regfile.write_as_type::<u8>(Vlmul::M2.get_lmul(), 2, &test_values);
        // store v2 to memory at an offset from BASE_ADDR
        vector_regfile
            .unit_stride_store(2, Vsew::E8, 1, store_addr, &mut mmio)
            .unwrap();
        // read back from memory and verify
        for i in 0..(VLEN_BYTE * 2) {
            let val = mmio.read_u8(store_addr + i as WordType).unwrap();
            assert_eq!(val, test_values[i], "M2 E8 mismatch at index {}", i);
        }

        // --------------- M1, E8 ---------------
        vector_regfile.config.vlmul = Vlmul::M1;
        vector_regfile.config.vl = VLEN_BYTE as u16;
        let test_values_m1: Vec<u8> = (0..VLEN_BYTE)
            .map(|i| (i.wrapping_mul(5).wrapping_add(7)) as u8)
            .collect();
        vector_regfile.write_as_type::<u8>(Vlmul::M1.get_lmul(), 4, &test_values_m1);
        vector_regfile
            .unit_stride_store(4, Vsew::E8, 1, store_addr, &mut mmio)
            .unwrap();
        for i in 0..VLEN_BYTE {
            let val = mmio.read_u8(store_addr + i as WordType).unwrap();
            assert_eq!(val, test_values_m1[i], "M1 E8 mismatch at index {}", i);
        }

        // --------------- verify that store didn't leak into BASE_ADDR ---------------
        let val_at_base = mmio.read_u8(BASE_ADDR).unwrap();
        assert_eq!(val_at_base, 0, "BASE_ADDR should remain untouched (0)");
    }
}
