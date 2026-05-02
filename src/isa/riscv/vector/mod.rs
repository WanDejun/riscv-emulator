use crate::{
    config::arch_config::WordType,
    cpu::VectorRegFile,
    device::{DeviceTrait, MemError, mmio::MemoryMapIO},
    isa::riscv::vector::types::{VGFRefMut, VectorConfig, Vlmul, Vsew},
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
        base + index << self.vsew as u8
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
    fn set_config(&mut self, lmul_sew_ta_ma_vl: (Vlmul, Vsew, bool, bool, u16)) {
        (
            self.config.vlmul,
            self.config.vsew,
            self.config.tail_agnostic,
            self.config.mask_agnostic,
            self.config.vl,
        ) = lmul_sew_ta_ma_vl;
    }

    pub(super) fn unit_stride_load(
        &mut self,
        vd: u8,
        eew: Vsew,
        seg: u8,
        base_addr: WordType,
        mem: &mut MemoryMapIO,
    ) -> Result<(), MemError> {
        let f = VectorUnitMemLoad { vsew: eew };
        let lmul = self.config.vlmul.get_lmul();
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut(lmul, vd, seg).unwrap(),
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
}
