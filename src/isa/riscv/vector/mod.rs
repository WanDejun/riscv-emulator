use crate::{
    config::arch_config::WordType,
    cpu::VectorRegFile,
    device::{DeviceTrait, MemError, mmio::MemoryMapIO},
    isa::riscv::{
        trap::Exception,
        vector::types::{VGFRef, VGFRefMut, VectorConfig, Vlmul, Vsew},
    },
};

pub mod integer;
pub mod types;
pub const VLEN: usize = 128;
pub const VLEN_BYTE: usize = VLEN >> 3;

/// Core structure of the RISC-V vector extension, managing vector configuration and the vector register file.
///
/// Provides execution capabilities for vector load/store instructions (stride, indexed, masked, etc.),
/// as well as configuration for vector length, SEW, LMUL, and agnostic policies.
pub(super) struct Vector {
    config: VectorConfig,
    vector_regfile: VectorRegFile,
}

// ============= Address Calculator =============

/// Stride address calculator for vector stride load/store instruction address generation.
///
/// Address formula: `base + index × stride`, where `stride` is fixed to the value specified by the instruction.
struct VectorStrideAddrCal {
    stride: WordType,
    base: WordType,
}

impl VectorStrideAddrCal {
    #[inline(always)]
    fn exec(&self, index: WordType) -> Result<WordType, MemError> {
        Ok(self.base + index * self.stride)
    }
}

/// Indexed address calculator for vector indexed load/store instruction address generation.
///
/// Address formula: `base + mem[index_arr_base + index × index_width]`,
/// i.e., the index array element is read from memory and used as the offset.
struct VectorIndexedAddrCal {
    index_arr_base: WordType,
    index_width: u8,
    base: WordType,
}

impl VectorIndexedAddrCal {
    #[inline(always)]
    fn exec<R>(&self, index: WordType, mem_reader: R) -> Result<WordType, MemError>
    where
        R: FnOnce(WordType, u32) -> Result<WordType, MemError>,
    {
        Ok(self.base
            + mem_reader(
                self.index_arr_base + self.index_width as WordType * index,
                self.index_width as u32,
            )?)
    }
}

/// Mask handler for vector load/store operations, managing mask, tail, and inactive element agnostic policies.
///
/// Based on the mask bits in the `v0` register and the `tail_agnostic` / `mask_agnostic` configuration,
/// determines whether each element actually performs memory access and whether unoperated elements
/// are written with the default value (agnostic).
struct VecOpMask {
    data: Vec<u8>,
    length: u16,
    enable_mask: bool,
    mask_agnostic: bool,
    tail_agnostic: bool,
}

impl VecOpMask {
    pub fn new(
        vgr: &VectorRegFile,
        length: u16,
        enable_mask: bool,
        mask_agnostic: bool,
        tail_agnostic: bool,
    ) -> Self {
        let data = vgr
            .read_as_type::<u8>(1, 0)
            .unwrap()
            .iter()
            .map(|v| *v)
            .collect();
        Self {
            data,
            length,
            enable_mask,
            mask_agnostic,
            tail_agnostic,
        }
    }

    #[inline(always)]
    fn bit(&self, index: usize) -> bool {
        let offset = index / 8;
        let inner_bit = 1 << (index % 8);
        let bit = (self.data[offset] & inner_bit) == inner_bit;
        (!self.enable_mask) || bit
    }

    #[inline(always)]
    fn mask<T>(&self, value: T, index: usize) -> Option<T>
    where
        T: Default,
    {
        let (mask, tail) = (self.bit(index), index >= self.length as usize);
        if mask && !tail {
            Some(value)
        } else if mask {
            if self.mask_agnostic {
                Some(T::default())
            } else {
                None
            }
        } else {
            if self.tail_agnostic {
                Some(T::default())
            } else {
                None
            }
        }
    }

    #[inline]
    pub fn element_load<T>(&self, element: types::RVVElemMutTy, value: T, index: usize)
    where
        T: Default,
    {
        match self.mask(value, index) {
            Some(v) => element.set(v),
            None => (),
        }
    }

    #[inline]
    pub fn element_store<T, F>(
        &self,
        store_fn: F,
        elem_value: types::RVVElemTy,
        index: usize,
    ) -> Result<(), MemError>
    where
        T: Default,
        F: FnOnce(T) -> Result<(), MemError>,
    {
        let value = elem_value.get::<T>();
        match self.mask(value, index) {
            Some(v) => store_fn(v),
            None => Ok(()),
        }
    }
}

#[inline(always)]
fn decode_whole_register_count(nf: u8) -> Result<u8, Exception> {
    match nf {
        0 => Ok(1),
        1 => Ok(2),
        3 => Ok(4),
        7 => Ok(8),
        _ => Err(Exception::IllegalInstruction),
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

    // This method will ignore segment argument, so DO NOT use this in load/store instruction.
    #[inline]
    #[cfg(test)]
    pub(super) fn read_as_type<T>(&self, idx: u8) -> Result<&[T], Exception> {
        self.vector_regfile
            .read_as_type(self.config.vlmul.get_lmul(), idx)
    }

    // This method will ignore segment argument, so DO NOT use this in load/store instruction.
    #[inline]
    #[cfg(test)]
    pub(super) fn write_as_type<T>(&mut self, lmul: u8, idx: u8, value: &[T]) {
        self.vector_regfile.write(lmul, idx, value, 1).unwrap();
    }

    #[inline]
    #[cfg(test)]
    pub(super) fn read_with_seg(
        &self,
        idx: u8,
        eew: Vsew,
        seg: u8,
    ) -> Result<VGFRef<'_>, Exception> {
        let lmul = self.config.vlmul.get_lmul();
        let raw = self.vector_regfile.get_ref(lmul, seg, idx)?;
        Ok(VGFRef::new(raw, eew.get_sew(), lmul, seg))
    }

    // ================= LOAD =================
    pub(super) fn stride_load(
        &mut self,
        vd: u8,
        eew: Vsew,
        seg: u8,
        stride: Option<WordType>,
        enable_mask: bool,
        base_addr: WordType,
        mem: &mut MemoryMapIO,
    ) -> Result<(), Exception> {
        let f = VectorStrideAddrCal {
            base: base_addr,
            stride: stride.unwrap_or(eew.get_sew() as WordType),
        };
        let lmul = self.config.vlmul.get_lmul();
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut(lmul, vd, seg)?,
            eew.get_sew(),
            lmul,
            seg,
        );
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl as u16 * seg as u16,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );

        let mut err = Ok(());
        // if set tail undisturbed, jump tail element
        vd_ref
            .iter_mut()
            .enumerate()
            .filter(|v| v.0 < self.config.vl as usize * seg as usize || self.config.tail_agnostic)
            .for_each(|(index, element)| {
                let addr = match f.exec(index as WordType) {
                    Ok(addr) => addr,
                    Err(_) => unreachable!(),
                };

                match eew {
                    Vsew::E8 => match mem.read_u8(addr) {
                        Ok(ram_value) => mask.element_load(element, ram_value, index),
                        Err(e) => err = Err(e),
                    },
                    Vsew::E16 => match mem.read_u16(addr) {
                        Ok(ram_value) => mask.element_load(element, ram_value, index),
                        Err(e) => err = Err(e),
                    },
                    Vsew::E32 => match mem.read_u32(addr) {
                        Ok(ram_value) => mask.element_load(element, ram_value, index),
                        Err(e) => err = Err(e),
                    },
                    Vsew::E64 => match mem.read_u64(addr) {
                        Ok(ram_value) => mask.element_load(element, ram_value, index),
                        Err(e) => err = Err(e),
                    },
                }
            });
        match err {
            Err(err) => Err(err.into()),
            Ok(()) => Ok(()),
        }
    }

    /// Execute vl[nf]r.v — whole vector register load, always unmasked.
    ///
    /// The raw `nf` field encodes the register count as:
    /// - nf=0 → vl1r.v (1 register)
    /// - nf=1 → vl2r.v (2 registers)
    /// - nf=3 → vl4r.v (4 registers)
    /// - nf=7 → vl8r.v (8 registers)
    ///
    /// The instruction always uses EEW=64, unit stride, ignores `vl` and `vtype`,
    /// and is always unmasked.
    pub(super) fn load_whole_register(
        &mut self,
        vd: u8,
        nf: u8,
        base_addr: WordType,
        mem: &mut MemoryMapIO,
    ) -> Result<(), Exception> {
        let eew = Vsew::E64;
        let f = VectorStrideAddrCal {
            base: base_addr,
            stride: eew.get_sew() as WordType,
        };
        let lmul = decode_whole_register_count(nf)?;

        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut(lmul, vd, 1)?,
            eew.get_sew(),
            lmul,
            1,
        );

        let mut err = Ok(());
        vd_ref.iter_mut().enumerate().for_each(|(index, element)| {
            let addr = match f.exec(index as WordType) {
                Ok(addr) => addr,
                Err(_) => unreachable!(),
            };

            match mem.read_u64(addr) {
                Ok(ram_value) => element.set(ram_value),
                Err(e) => err = Err(e),
            };
        });
        match err {
            Err(err) => Err(err.into()),
            Ok(()) => Ok(()),
        }
    }

    pub(super) fn indexed_ordered_load(
        &mut self,
        vd: u8,
        eew: Vsew,
        seg: u8,
        index_arr_base: WordType,
        enable_mask: bool,
        base_addr: WordType,
        mem: &mut MemoryMapIO,
    ) -> Result<(), Exception> {
        let f = VectorIndexedAddrCal {
            base: base_addr,
            index_arr_base,
            index_width: eew.get_sew(),
        };
        let lmul = self.config.vlmul.get_lmul();
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut(lmul, vd, seg)?,
            self.config.vsew.get_sew(),
            lmul,
            seg,
        );
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl as u16 * seg as u16,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );

        let mut err = Ok(());
        // if set tail undisturbed, jump tail element
        vd_ref
            .iter_mut()
            .enumerate()
            .filter(|v| v.0 < self.config.vl as usize * seg as usize || self.config.tail_agnostic)
            .for_each(|(index, element)| {
                let addr = match f.exec(index as WordType, |addr, len| mem.read(addr, len)) {
                    Ok(addr) => addr,
                    Err(e) => {
                        err = Err(e);
                        return;
                    }
                };

                match self.config.vsew {
                    Vsew::E8 => match mem.read_u8(addr) {
                        Ok(ram_value) => mask.element_load(element, ram_value, index),
                        Err(e) => err = Err(e),
                    },
                    Vsew::E16 => match mem.read_u16(addr) {
                        Ok(ram_value) => mask.element_load(element, ram_value, index),
                        Err(e) => err = Err(e),
                    },
                    Vsew::E32 => match mem.read_u32(addr) {
                        Ok(ram_value) => mask.element_load(element, ram_value, index),
                        Err(e) => err = Err(e),
                    },
                    Vsew::E64 => match mem.read_u64(addr) {
                        Ok(ram_value) => mask.element_load(element, ram_value, index),
                        Err(e) => err = Err(e),
                    },
                }
            });
        match err {
            Err(err) => Err(err.into()),
            Ok(()) => Ok(()),
        }
    }

    // ================= STORE =================
    pub(super) fn stride_store(
        &mut self,
        vs: u8,
        eew: Vsew,
        seg: u8,
        stride: Option<WordType>,
        enable_mask: bool,
        base_addr: WordType,
        mem: &mut MemoryMapIO,
    ) -> Result<(), Exception> {
        let f = VectorStrideAddrCal {
            base: base_addr,
            stride: stride.unwrap_or(eew.get_sew() as WordType),
        };
        let lmul = self.config.vlmul.get_lmul();
        let vd_ref = VGFRef::new(
            self.vector_regfile.get_ref(lmul, seg, vs)?,
            eew.get_sew(),
            lmul,
            seg,
        );
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl as u16 * seg as u16,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );

        let mut err = Ok(());
        // if set tail undisturbed, jump tail element
        vd_ref
            .iter()
            .enumerate()
            .filter(|v| v.0 < self.config.vl as usize * seg as usize || self.config.tail_agnostic)
            .for_each(|(index, element)| {
                let addr = match f.exec(index as WordType) {
                    Ok(addr) => addr,
                    Err(e) => {
                        err = Err(e);
                        return;
                    }
                };
                match eew {
                    Vsew::E8 => mask
                        .element_store(|v| mem.write_u8(addr, v), element, index)
                        .unwrap_or_else(|e| err = Err(e.into())),
                    Vsew::E16 => mask
                        .element_store(|v| mem.write_u16(addr, v), element, index)
                        .unwrap_or_else(|e| err = Err(e.into())),
                    Vsew::E32 => mask
                        .element_store(|v| mem.write_u32(addr, v), element, index)
                        .unwrap_or_else(|e| err = Err(e.into())),
                    Vsew::E64 => mask
                        .element_store(|v| mem.write_u64(addr, v), element, index)
                        .unwrap_or_else(|e| err = Err(e.into())),
                }
            });
        match err {
            Err(err) => Err(err.into()),
            Ok(()) => Ok(()),
        }
    }

    pub(super) fn indexed_ordered_store(
        &mut self,
        vs: u8,
        eew: Vsew,
        seg: u8,
        index_arr_base: WordType,
        enable_mask: bool,
        base_addr: WordType,
        mem: &mut MemoryMapIO,
    ) -> Result<(), Exception> {
        let f = VectorIndexedAddrCal {
            base: base_addr,
            index_arr_base,
            index_width: eew.get_sew(),
        };
        let lmul = self.config.vlmul.get_lmul();
        let vd_ref = VGFRef::new(
            self.vector_regfile.get_ref(lmul, seg, vs)?,
            self.config.vsew.get_sew(),
            lmul,
            seg,
        );
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl as u16 * seg as u16,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );

        let mut err = Ok(());
        // if set tail undisturbed, jump tail element
        vd_ref
            .iter()
            .enumerate()
            .filter(|v| v.0 < self.config.vl as usize * seg as usize || self.config.tail_agnostic)
            .for_each(|(index, element)| {
                let addr = match f.exec(index as WordType, |addr, len| mem.read(addr, len)) {
                    Ok(addr) => addr,
                    Err(e) => {
                        err = Err(e);
                        return;
                    }
                };
                match self.config.vsew {
                    Vsew::E8 => mask
                        .element_store(|v| mem.write_u8(addr, v), element, index)
                        .unwrap_or_else(|e| err = Err(e.into())),
                    Vsew::E16 => mask
                        .element_store(|v| mem.write_u16(addr, v), element, index)
                        .unwrap_or_else(|e| err = Err(e.into())),
                    Vsew::E32 => mask
                        .element_store(|v| mem.write_u32(addr, v), element, index)
                        .unwrap_or_else(|e| err = Err(e.into())),
                    Vsew::E64 => mask
                        .element_store(|v| mem.write_u64(addr, v), element, index)
                        .unwrap_or_else(|e| err = Err(e.into())),
                }
            });
        match err {
            Err(err) => Err(err.into()),
            Ok(()) => Ok(()),
        }
    }

    /// Execute vs[nf]r.v — whole vector register store, always unmasked.
    ///
    /// The raw `nf` field encodes the register count as:
    /// - nf=0 → vs1r.v (1 register)
    /// - nf=1 → vs2r.v (2 registers)
    /// - nf=3 → vs4r.v (4 registers)
    /// - nf=7 → vs8r.v (8 registers)
    ///
    /// The instruction always uses EEW=64, unit stride, ignores `vl` and `vtype`,
    /// and is always unmasked.
    pub(super) fn store_whole_register(
        &mut self,
        vs: u8,
        nf: u8,
        base_addr: WordType,
        mem: &mut MemoryMapIO,
    ) -> Result<(), Exception> {
        let eew = Vsew::E64;
        let f = VectorStrideAddrCal {
            base: base_addr,
            stride: eew.get_sew() as WordType,
        };
        let lmul = decode_whole_register_count(nf)?;
        let vs_ref = VGFRef::new(
            self.vector_regfile.get_ref(lmul, 1, vs)?,
            eew.get_sew(),
            lmul,
            1,
        );

        let mut err = Ok(());
        vs_ref.iter().enumerate().for_each(|(index, element)| {
            let addr = match f.exec(index as WordType) {
                Ok(addr) => addr,
                Err(_) => unreachable!(),
            };

            if let Err(e) = mem.write_u64(addr, element.get::<u64>()) {
                err = Err(e);
            }
        });
        match err {
            Err(err) => Err(err.into()),
            Ok(()) => Ok(()),
        }
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
            .stride_load(2, Vsew::E8, 1, None, false, BASE_ADDR, &mut mmio)
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
        vector_regfile.config.vl = VLEN_BYTE as u16 * Vlmul::M1.get_lmul() as u16;
        vector_regfile
            .stride_load(2, Vsew::E8, 2, None, false, BASE_ADDR, &mut mmio)
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
            .stride_store(2, Vsew::E8, 1, None, false, store_addr, &mut mmio)
            .unwrap();
        // read back from memory and verify
        for i in 0..(VLEN_BYTE * 2) {
            let val = mmio.read_u8(store_addr + i as WordType).unwrap();
            assert_eq!(val, test_values[i], "M2 E8 mismatch at index {}", i);
        }

        // --------------- M2, E32, Seg=4 ---------------
        const SEG_SIZE: u8 = 4;
        const LMUL: Vlmul = Vlmul::M2;
        vector_regfile.config.vlmul = LMUL;
        vector_regfile.config.vl = VLEN_BYTE as u16;
        let elems_per_seg = VLEN_BYTE * LMUL.get_lmul() as usize / size_of::<u32>();
        let total_elements = elems_per_seg * SEG_SIZE as usize;
        let test_values_seg: Vec<u32> = (0..total_elements)
            .map(|i| (i.wrapping_add(1)) as u32)
            .collect();
        // Write 4 M2 register groups: v0-v1, v2-v3, v4-v5, v6-v7
        for seg_idx in 0..SEG_SIZE as usize {
            let start = seg_idx * elems_per_seg;
            let end = start + elems_per_seg;
            vector_regfile.write_as_type::<u32>(
                LMUL.get_lmul(),
                (LMUL.get_lmul() as usize * seg_idx) as u8,
                &test_values_seg[start..end],
            );
        }
        vector_regfile
            .stride_store(0, Vsew::E32, SEG_SIZE, None, false, store_addr, &mut mmio)
            .unwrap();
        // Verify: segment-interleaved layout in memory
        for pos in 0..elems_per_seg {
            for seg_idx in 0..SEG_SIZE as usize {
                let idx = seg_idx * elems_per_seg + pos;
                let addr = store_addr
                    + ((pos * SEG_SIZE as usize + seg_idx) * size_of::<u32>()) as WordType;
                let val = mmio.read_u32(addr).unwrap();
                assert_eq!(
                    val, test_values_seg[idx],
                    "M2 E32 Seg={} mismatch at pos {}, seg {}",
                    SEG_SIZE, pos, seg_idx
                );
            }
        }

        // --------------- verify that store didn't leak into BASE_ADDR ---------------
        let val_at_base = mmio.read_u8(BASE_ADDR).unwrap();
        assert_eq!(val_at_base, 0, "BASE_ADDR should remain untouched (0)");
    }

    #[test]
    fn test_indexed_ordered_load() {
        let mut vector_regfile = Vector::new();
        let mut ram = Ram::new();
        let index_arr_base = BASE_ADDR + 0x1000;
        let data_base = BASE_ADDR + 0x2000;

        // 索引数组（u32）: [0, 4, 8, ...]
        for i in 0..VLEN_BYTE {
            ram.write(0x1000 + (i * size_of::<u32>()) as WordType, (i * 4) as u32)
                .unwrap();
        }
        // 数据区（u32）: value = i + 100
        for i in 0..VLEN_BYTE {
            ram.write(0x2000 + (i * 4) as WordType, (i as u32) + 100)
                .unwrap();
        }

        let mut mmio = MemoryMapIO::from_mmio_items(Rc::new(UnsafeCell::new(ram)), vec![]);

        vector_regfile.config.vlmul = Vlmul::M1;
        vector_regfile.config.vsew = Vsew::E32;
        vector_regfile.config.vl = (VLEN_BYTE / size_of::<u32>()) as u16;

        vector_regfile
            .indexed_ordered_load(0, Vsew::E32, 1, index_arr_base, false, data_base, &mut mmio)
            .unwrap();

        let vector_ref = vector_regfile
            .vector_regfile
            .read_as_type::<u32>(Vlmul::M1.get_lmul(), 0)
            .unwrap();
        for i in 0..(VLEN_BYTE / size_of::<u32>()) {
            assert_eq!(
                vector_ref[i],
                i as u32 + 100,
                "indexed load mismatch at {}",
                i
            );
        }
    }

    #[test]
    fn test_indexed_ordered_store() {
        let mut vector_regfile = Vector::new();
        let ram = Ram::new();
        let index_arr_base = BASE_ADDR + 0x1000;
        let data_base = BASE_ADDR + 0x2000;
        let mut mmio = MemoryMapIO::from_mmio_items(Rc::new(UnsafeCell::new(ram)), vec![]);

        // 索引数组（u32）: [0, 4, 8, ...]
        for i in 0..VLEN_BYTE {
            mmio.write_u32(
                index_arr_base + (i * size_of::<u32>()) as WordType,
                (i * 4) as u32,
            )
            .unwrap();
        }

        vector_regfile.config.vlmul = Vlmul::M1;
        vector_regfile.config.vsew = Vsew::E32;
        vector_regfile.config.vl = (VLEN_BYTE / size_of::<u32>()) as u16;

        let element_count = VLEN_BYTE / size_of::<u32>();
        let test_values: Vec<u32> = (0..element_count).map(|i| (i as u32) * 7 + 11).collect();
        vector_regfile.write_as_type::<u32>(Vlmul::M1.get_lmul(), 0, &test_values);

        vector_regfile
            .indexed_ordered_store(0, Vsew::E32, 1, index_arr_base, false, data_base, &mut mmio)
            .unwrap();

        for i in 0..element_count {
            let addr = data_base + (i * 4) as WordType;
            let val = mmio.read_u32(addr).unwrap();
            assert_eq!(val, test_values[i], "indexed store mismatch at index {}", i);
        }
    }

    #[test]
    fn test_mask_unit_stride_load() {
        let mut vector_regfile = Vector::new();
        let mut ram = Ram::new();
        let addr_offset = 0x2000;
        let base_addr = BASE_ADDR + addr_offset;

        type ElemType = u32;
        const LMUL: Vlmul = Vlmul::M4;
        const SEW: Vsew = Vsew::E32;
        let elem_cnt = VLEN_BYTE * LMUL.get_lmul() as usize / size_of::<ElemType>();

        for i in 0..elem_cnt {
            ram.write(
                addr_offset + (i * size_of::<ElemType>()) as WordType,
                (i as u32) + 100,
            )
            .unwrap();
        }
        let mut mmio = MemoryMapIO::from_mmio_items(Rc::new(UnsafeCell::new(ram)), vec![]);

        vector_regfile.set_config((LMUL, SEW, false, false, elem_cnt as u16));

        // mask register v0: bit=1 时生效。这里设置偶数位为 1、奇数位为 0。
        let mut mask_bytes = [0u8; VLEN_BYTE];
        for i in 0..elem_cnt {
            if i % 2 == 0 {
                mask_bytes[i / 8] |= 1 << (i % 8);
            }
        }
        vector_regfile.write_as_type::<u8>(1, 0, &mask_bytes);

        let init = vec![0xDEAD_BEEF_u32; elem_cnt];
        vector_regfile.write_as_type::<ElemType>(LMUL.get_lmul(), 8, &init);

        vector_regfile
            .stride_load(8, SEW, 1, None, true, base_addr, &mut mmio)
            .unwrap();

        let got = vector_regfile
            .vector_regfile
            .read_as_type::<ElemType>(LMUL.get_lmul(), 8)
            .unwrap();
        for i in 0..elem_cnt {
            let expected = if i % 2 == 0 {
                (i as u32) + 100
            } else {
                0xDEAD_BEEF_u32
            };
            assert_eq!(got[i], expected, "mask load mismatch at index {}", i);
        }
    }

    #[test]
    fn test_mask_unit_stride_store() {
        let mut vector_regfile = Vector::new();
        let mut ram = Ram::new();
        let addr_offset = 0x2000;
        let base_addr = BASE_ADDR + 0x2000;

        type ElemType = u32;
        const LMUL: Vlmul = Vlmul::M4;
        const SEW: Vsew = Vsew::E32;
        let elem_cnt = VLEN_BYTE * LMUL.get_lmul() as usize / size_of::<ElemType>();

        // 预置内存为 0，便于检查被 mask 掉的元素不会被写入。
        for i in 0..elem_cnt {
            ram.write(addr_offset + (i * size_of::<ElemType>()) as WordType, 0u32)
                .unwrap();
        }
        let mut mmio = MemoryMapIO::from_mmio_items(Rc::new(UnsafeCell::new(ram)), vec![]);

        vector_regfile.set_config((LMUL, SEW, false, false, elem_cnt as u16));

        // mask register v0: 仅偶数位写入
        let mut mask_bytes = [0u8; VLEN_BYTE];
        for i in 0..elem_cnt {
            if i % 2 == 0 {
                mask_bytes[i / 8] |= 1 << (i % 8);
            }
        }
        vector_regfile.write_as_type::<u8>(1, 0, &mask_bytes);

        let src: Vec<ElemType> = (0..elem_cnt).map(|i| (i as u32) * 11 + 7).collect();
        vector_regfile.write_as_type::<ElemType>(LMUL.get_lmul(), 8, &src);

        vector_regfile
            .stride_store(8, SEW, 1, None, true, base_addr, &mut mmio)
            .unwrap();

        for i in 0..elem_cnt {
            let addr = base_addr + (i * size_of::<ElemType>()) as WordType;
            let got = mmio.read_u32(addr).unwrap();
            let expected = if i % 2 == 0 { src[i] } else { 0u32 };
            assert_eq!(got, expected, "mask store mismatch at index {}", i);
        }
    }

    #[test]
    fn test_load_whole_register() {
        let mut vector_regfile = Vector::new();
        let mut ram = Ram::new();
        let base_addr = BASE_ADDR + 0x3000;
        let test_values: Vec<u64> = (0..(VLEN_BYTE * 8 / size_of::<u64>()))
            .map(|i| 0x1000_0000_0000_0000u64 + i as u64)
            .collect();

        for (i, value) in test_values.iter().copied().enumerate() {
            ram.write(
                base_addr - BASE_ADDR + (i * size_of::<u64>()) as WordType,
                value,
            )
            .unwrap();
        }

        let mut mmio = MemoryMapIO::from_mmio_items(Rc::new(UnsafeCell::new(ram)), vec![]);
        vector_regfile.set_config((Vlmul::M1, Vsew::E8, true, true, 1));

        vector_regfile
            .load_whole_register(8, 7, base_addr, &mut mmio)
            .unwrap();

        let vector_ref = vector_regfile
            .vector_regfile
            .read_as_type::<u64>(8, 8)
            .unwrap();
        assert_eq!(vector_ref, test_values.as_slice());
    }

    #[test]
    fn test_store_whole_register() {
        let mut vector_regfile = Vector::new();
        let ram = Ram::new();
        let base_addr = BASE_ADDR + 0x4000;
        let mut mmio = MemoryMapIO::from_mmio_items(Rc::new(UnsafeCell::new(ram)), vec![]);
        let test_values: Vec<u64> = (0..(VLEN_BYTE * 8 / size_of::<u64>()))
            .map(|i| 0x2000_0000_0000_0000u64 + (i as u64) * 3)
            .collect();

        vector_regfile.set_config((Vlmul::M1, Vsew::E8, true, true, 1));
        vector_regfile.write_as_type::<u64>(8, 8, &test_values);

        vector_regfile
            .store_whole_register(8, 7, base_addr, &mut mmio)
            .unwrap();

        for (i, expected) in test_values.iter().copied().enumerate() {
            let got = mmio
                .read_u64(base_addr + (i * size_of::<u64>()) as WordType)
                .unwrap();
            assert_eq!(
                got, expected,
                "whole register store mismatch at index {}",
                i
            );
        }
    }
}
