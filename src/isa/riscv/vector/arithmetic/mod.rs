use crate::{
    config::arch_config::WordType,
    isa::riscv::{
        instruction::exec_function::ExecUnaryTrait,
        trap::Exception,
        vector::{
            VLEN_BYTE, VecOpMask, Vector,
            types::{VGFRef, VGFRefMut, Vsew},
        },
    },
};

pub(in crate::isa::riscv) mod integer_impl;
pub(in crate::isa::riscv) use integer_impl::*;

impl Vector {
    #[inline]
    pub(in crate::isa::riscv) fn exec_integer_vv<OpIVV>(
        &mut self,
        vs1: u8,
        vs2: u8,
        vd: u8,
        enable_mask: bool,
    ) -> Result<(), Exception>
    where
        OpIVV: VectorOpIntegerVV,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (sew, lmul) = (vsew.into_byte_width(), vlmul.get_lmul());
        let vs1_data = self.vector_regfile.get_ref(lmul, 1, vs1)?.to_vec();
        let vs2_data = self.vector_regfile.get_ref(lmul, 1, vs2)?.to_vec();
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );
        let vs1_ref = VGFRef::new(&vs1_data, sew, lmul, 1);
        let vs2_ref = VGFRef::new(&vs2_data, sew, lmul, 1);
        let mut vd_ref = VGFRefMut::new(self.vector_regfile.get_mut(lmul, vd, 1)?, sew, lmul, 1);
        OpIVV::exec(&vs1_ref, &vs2_ref, &mut vd_ref, &mask)
    }

    #[inline]
    pub(in crate::isa::riscv) fn exec_integer_vx<OpIVX>(
        &mut self,
        x1: WordType,
        vs2: u8,
        vd: u8,
        enable_mask: bool,
    ) -> Result<(), Exception>
    where
        OpIVX: VectorOpIntegerVX,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (sew, lmul) = (vsew.into_byte_width(), vlmul.get_lmul());
        let vs2_data = self.vector_regfile.get_ref(lmul, 1, vs2)?.to_vec();
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );
        let vs2_ref = VGFRef::new(&vs2_data, sew, lmul, 1);
        let mut vd_ref = VGFRefMut::new(self.vector_regfile.get_mut(lmul, vd, 1)?, sew, lmul, 1);
        OpIVX::exec(x1, &vs2_ref, &mut vd_ref, &mask)
    }

    #[inline]
    pub(in crate::isa::riscv) fn exec_integer_vvv<OpIVV>(
        &mut self,
        vs1: u8,
        vs2: u8,
        vd: u8,
        enable_mask: bool,
    ) -> Result<(), Exception>
    where
        OpIVV: VectorOpIntegerVVV,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (sew, lmul) = (vsew.into_byte_width(), vlmul.get_lmul());
        let vs1_data = self.vector_regfile.get_ref(lmul, 1, vs1)?.to_vec();
        let vs2_data = self.vector_regfile.get_ref(lmul, 1, vs2)?.to_vec();
        let vd_data = self.vector_regfile.get_ref(lmul, 1, vd)?.to_vec();
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );
        let vs1_ref = VGFRef::new(&vs1_data, sew, lmul, 1);
        let vs2_ref = VGFRef::new(&vs2_data, sew, lmul, 1);
        let old_vd_ref = VGFRef::new(&vd_data, sew, lmul, 1);
        let mut vd_ref = VGFRefMut::new(self.vector_regfile.get_mut(lmul, vd, 1)?, sew, lmul, 1);
        OpIVV::exec(&vs1_ref, &vs2_ref, &old_vd_ref, &mut vd_ref, &mask)
    }

    #[inline]
    pub(in crate::isa::riscv) fn exec_integer_vxv<OpIVX>(
        &mut self,
        x1: WordType,
        vs2: u8,
        vd: u8,
        enable_mask: bool,
    ) -> Result<(), Exception>
    where
        OpIVX: VectorOpIntegerVXV,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (sew, lmul) = (vsew.into_byte_width(), vlmul.get_lmul());
        let vs2_data = self.vector_regfile.get_ref(lmul, 1, vs2)?.to_vec();
        let vd_data = self.vector_regfile.get_ref(lmul, 1, vd)?.to_vec();
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );
        let vs2_ref = VGFRef::new(&vs2_data, sew, lmul, 1);
        let old_vd_ref = VGFRef::new(&vd_data, sew, lmul, 1);
        let mut vd_ref = VGFRefMut::new(self.vector_regfile.get_mut(lmul, vd, 1)?, sew, lmul, 1);
        OpIVX::exec(x1, &vs2_ref, &old_vd_ref, &mut vd_ref, &mask)
    }

    #[inline]
    pub(in crate::isa::riscv) fn exec_widening_integer_vv<OpIVV>(
        &mut self,
        vs1: u8,
        vs2: u8,
        vd: u8,
        enable_mask: bool,
    ) -> Result<(), Exception>
    where
        OpIVV: VectorOpWideningIntegerVV,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (src_sew, src_lmul) = (vsew.into_byte_width(), vlmul.get_lmul());
        let Some(dst_eew) = Vsew::from_byte_width(src_sew * 2) else {
            return Err(Exception::IllegalInstruction);
        };
        let dst_lmul = src_lmul * 2;
        if dst_lmul > 8 {
            return Err(Exception::IllegalInstruction);
        }

        let vs1_data = self.vector_regfile.get_ref(src_lmul, 1, vs1)?.to_vec();
        let vs2_data = self.vector_regfile.get_ref(src_lmul, 1, vs2)?.to_vec();
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );
        let vs1_ref = VGFRef::new(&vs1_data, src_sew, src_lmul, 1);
        let vs2_ref = VGFRef::new(&vs2_data, src_sew, src_lmul, 1);
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut(dst_lmul, vd, 1)?,
            dst_eew.into_byte_width(),
            dst_lmul,
            1,
        );
        OpIVV::exec(&vs1_ref, &vs2_ref, &mut vd_ref, &mask)
    }

    #[inline]
    pub(in crate::isa::riscv) fn exec_widening_integer_vvv<OpIVV>(
        &mut self,
        vs1: u8,
        vs2: u8,
        vd: u8,
        enable_mask: bool,
    ) -> Result<(), Exception>
    where
        OpIVV: VectorOpWideningIntegerVVV,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (src_sew, src_lmul) = (vsew.into_byte_width(), vlmul.get_lmul());
        let Some(dst_eew) = Vsew::from_byte_width(src_sew * 2) else {
            return Err(Exception::IllegalInstruction);
        };
        let dst_lmul = src_lmul * 2;
        if dst_lmul > 8 {
            return Err(Exception::IllegalInstruction);
        }

        let vs1_data = self.vector_regfile.get_ref(src_lmul, 1, vs1)?.to_vec();
        let vs2_data = self.vector_regfile.get_ref(src_lmul, 1, vs2)?.to_vec();
        let vd_data = self.vector_regfile.get_ref(dst_lmul, 1, vd)?.to_vec();
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );
        let vs1_ref = VGFRef::new(&vs1_data, src_sew, src_lmul, 1);
        let vs2_ref = VGFRef::new(&vs2_data, src_sew, src_lmul, 1);
        let old_vd_ref = VGFRef::new(&vd_data, dst_eew.into_byte_width(), dst_lmul, 1);
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut(dst_lmul, vd, 1)?,
            dst_eew.into_byte_width(),
            dst_lmul,
            1,
        );
        OpIVV::exec(&vs1_ref, &vs2_ref, &old_vd_ref, &mut vd_ref, &mask)
    }

    #[inline]
    pub(in crate::isa::riscv) fn exec_widening_integer_vxv<OpIVX>(
        &mut self,
        x1: WordType,
        vs2: u8,
        vd: u8,
        enable_mask: bool,
    ) -> Result<(), Exception>
    where
        OpIVX: VectorOpWideningIntegerVXV,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (src_sew, src_lmul) = (vsew.into_byte_width(), vlmul.get_lmul());
        let Some(dst_eew) = Vsew::from_byte_width(src_sew * 2) else {
            return Err(Exception::IllegalInstruction);
        };
        let dst_lmul = src_lmul * 2;
        if dst_lmul > 8 {
            return Err(Exception::IllegalInstruction);
        }

        let vs2_data = self.vector_regfile.get_ref(src_lmul, 1, vs2)?.to_vec();
        let vd_data = self.vector_regfile.get_ref(dst_lmul, 1, vd)?.to_vec();
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );
        let vs2_ref = VGFRef::new(&vs2_data, src_sew, src_lmul, 1);
        let old_vd_ref = VGFRef::new(&vd_data, dst_eew.into_byte_width(), dst_lmul, 1);
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut(dst_lmul, vd, 1)?,
            dst_eew.into_byte_width(),
            dst_lmul,
            1,
        );
        OpIVX::exec(x1, &vs2_ref, &old_vd_ref, &mut vd_ref, &mask)
    }

    #[inline]
    pub(in crate::isa::riscv) fn exec_widening_integer_vx<OpIVX>(
        &mut self,
        x1: WordType,
        vs2: u8,
        vd: u8,
        enable_mask: bool,
    ) -> Result<(), Exception>
    where
        OpIVX: VectorOpWideningIntegerVX,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (src_sew, src_lmul) = (vsew.into_byte_width(), vlmul.get_lmul());
        let Some(dst_eew) = Vsew::from_byte_width(src_sew * 2) else {
            return Err(Exception::IllegalInstruction);
        };
        let dst_lmul = src_lmul * 2;
        if dst_lmul > 8 {
            return Err(Exception::IllegalInstruction);
        }

        let vs2_data = self.vector_regfile.get_ref(src_lmul, 1, vs2)?.to_vec();
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );
        let vs2_ref = VGFRef::new(&vs2_data, src_sew, src_lmul, 1);
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut(dst_lmul, vd, 1)?,
            dst_eew.into_byte_width(),
            dst_lmul,
            1,
        );
        OpIVX::exec(x1, &vs2_ref, &mut vd_ref, &mask)
    }

    #[inline]
    pub(in crate::isa::riscv) fn exec_widening_integer_wv<OpIVV>(
        &mut self,
        vs1: u8,
        vs2: u8,
        vd: u8,
        enable_mask: bool,
    ) -> Result<(), Exception>
    where
        OpIVV: VectorOpWideningIntegerWV,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (src_sew, src_lmul) = (vsew.into_byte_width(), vlmul.get_lmul());
        let Some(dst_eew) = Vsew::from_byte_width(src_sew * 2) else {
            return Err(Exception::IllegalInstruction);
        };
        let dst_lmul = src_lmul * 2;
        if dst_lmul > 8 {
            return Err(Exception::IllegalInstruction);
        }

        let vs1_data = self.vector_regfile.get_ref(src_lmul, 1, vs1)?.to_vec();
        let vs2_data = self.vector_regfile.get_ref(dst_lmul, 1, vs2)?.to_vec();
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );
        let vs1_ref = VGFRef::new(&vs1_data, src_sew, src_lmul, 1);
        let vs2_ref = VGFRef::new(&vs2_data, dst_eew.into_byte_width(), dst_lmul, 1);
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut(dst_lmul, vd, 1)?,
            dst_eew.into_byte_width(),
            dst_lmul,
            1,
        );
        OpIVV::exec(&vs1_ref, &vs2_ref, &mut vd_ref, &mask)
    }

    #[inline]
    pub(in crate::isa::riscv) fn exec_widening_integer_wx<OpIVX>(
        &mut self,
        x1: WordType,
        vs2: u8,
        vd: u8,
        enable_mask: bool,
    ) -> Result<(), Exception>
    where
        OpIVX: VectorOpWideningIntegerWX,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (src_sew, src_lmul) = (vsew.into_byte_width(), vlmul.get_lmul());
        let Some(dst_eew) = Vsew::from_byte_width(src_sew * 2) else {
            return Err(Exception::IllegalInstruction);
        };
        let dst_lmul = src_lmul * 2;
        if dst_lmul > 8 {
            return Err(Exception::IllegalInstruction);
        }

        let vs2_data = self.vector_regfile.get_ref(dst_lmul, 1, vs2)?.to_vec();
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );
        let vs2_ref = VGFRef::new(&vs2_data, dst_eew.into_byte_width(), dst_lmul, 1);
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut(dst_lmul, vd, 1)?,
            dst_eew.into_byte_width(),
            dst_lmul,
            1,
        );
        OpIVX::exec(x1, &vs2_ref, &mut vd_ref, &mask)
    }

    #[inline]
    pub(in crate::isa::riscv) fn exec_integer_v<OpIV>(
        &mut self,
        vs2: u8,
        vd: u8,
        src_eew: Vsew,
        dst_eew: Vsew,
        enable_mask: bool,
    ) -> Result<(), Exception>
    where
        OpIV: VectorOpIntegerV,
    {
        let lmul = self.config.vlmul.get_lmul();
        let vs2_data = self.vector_regfile.get_ref(lmul, 1, vs2)?.to_vec();
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );
        let vs2_ref = VGFRef::new(&vs2_data, src_eew.into_byte_width(), lmul, 1);
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut(lmul, vd, 1)?,
            dst_eew.into_byte_width(),
            lmul,
            1,
        );
        OpIV::exec(&vs2_ref, &mut vd_ref, &mask)
    }

    #[inline]
    pub(in crate::isa::riscv) fn exec_integer_v_ext<OpIV, const FACTOR: u8>(
        &mut self,
        vs2: u8,
        vd: u8,
        enable_mask: bool,
    ) -> Result<(), Exception>
    where
        OpIV: VectorOpIntegerV,
    {
        let dst_eew = self.config.vsew;
        let Some(src_eew) = Vsew::from_byte_width(dst_eew.into_byte_width() / FACTOR) else {
            return Err(Exception::IllegalInstruction);
        };
        self.exec_integer_v::<OpIV>(vs2, vd, src_eew, dst_eew, enable_mask)
    }

    #[inline]
    pub(in crate::isa::riscv) fn exec_integer_scalar_move<OpIV, T>(
        &mut self,
        src: T,
        vd: u8,
    ) -> Result<(), Exception>
    where
        OpIV: ExecUnaryTrait<Result<T, Exception>, T>,
        T: Copy + Default,
    {
        let lmul = self.config.vlmul.get_lmul();
        let Some(eew) = Vsew::from_byte_width(size_of::<T>() as u8) else {
            return Err(Exception::IllegalInstruction);
        };
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl,
            false,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut(lmul, vd, 1)?,
            eew.into_byte_width(),
            lmul,
            1,
        );
        for (index, element) in vd_ref.iter_mut().enumerate() {
            mask.element_load(element, OpIV::exec(src)?, index);
        }

        Ok(())
    }

    #[inline]
    pub(in crate::isa::riscv) fn exec_whole_register_move<OpIV>(
        &mut self,
        vs2: u8,
        vd: u8,
        lmul: u8,
    ) -> Result<(), Exception>
    where
        OpIV: VectorOpIntegerV,
    {
        let vs2_data = self.vector_regfile.get_ref(lmul, 1, vs2)?.to_vec();
        let mask = VecOpMask::new(
            &self.vector_regfile,
            (VLEN_BYTE * lmul as usize / size_of::<u64>()) as u16,
            false,
            false,
            false,
        );
        let vs2_ref = VGFRef::new(&vs2_data, Vsew::E64.into_byte_width(), lmul, 1);
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut(lmul, vd, 1)?,
            Vsew::E64.into_byte_width(),
            lmul,
            1,
        );
        OpIV::exec(&vs2_ref, &mut vd_ref, &mask)
    }

    #[inline]
    pub(in crate::isa::riscv) fn exec_integer_vvm<OpIVVM>(
        &mut self,
        vs1: u8,
        vs2: u8,
        v0: u8,
        vd: u8,
        enable_mask: bool,
    ) -> Result<(), Exception>
    where
        OpIVVM: VectorOpIntegerVVM,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (sew, lmul) = (vsew.into_byte_width(), vlmul.get_lmul());
        let vs1_data = self.vector_regfile.get_ref(lmul, 1, vs1)?.to_vec();
        let vs2_data = self.vector_regfile.get_ref(lmul, 1, vs2)?.to_vec();
        let v0_data = self.vector_regfile.get_ref(1, 1, v0)?.to_vec();
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );
        let vs1_ref = VGFRef::new(&vs1_data, sew, lmul, 1);
        let vs2_ref = VGFRef::new(&vs2_data, sew, lmul, 1);
        let v0_ref = VGFRef::new(&v0_data, Vsew::E8.into_byte_width(), 1, 1);
        let mut vd_ref = VGFRefMut::new(self.vector_regfile.get_mut(lmul, vd, 1)?, sew, lmul, 1);
        OpIVVM::exec(&vs1_ref, &vs2_ref, &v0_ref, &mut vd_ref, &mask)
    }

    #[inline]
    pub(in crate::isa::riscv) fn exec_integer_vxm<OpIVXM>(
        &mut self,
        x1: WordType,
        vs2: u8,
        v0: u8,
        vd: u8,
        enable_mask: bool,
    ) -> Result<(), Exception>
    where
        OpIVXM: VectorOpIntegerVXM,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (sew, lmul) = (vsew.into_byte_width(), vlmul.get_lmul());
        let vs2_data = self.vector_regfile.get_ref(lmul, 1, vs2)?.to_vec();
        let v0_data = self.vector_regfile.get_ref(1, 1, v0)?.to_vec();
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );
        let vs2_ref = VGFRef::new(&vs2_data, sew, lmul, 1);
        let v0_ref = VGFRef::new(&v0_data, Vsew::E8.into_byte_width(), 1, 1);
        let mut vd_ref = VGFRefMut::new(self.vector_regfile.get_mut(lmul, vd, 1)?, sew, lmul, 1);
        OpIVXM::exec(x1, &vs2_ref, &v0_ref, &mut vd_ref, &mask)
    }

    #[inline]
    pub(in crate::isa::riscv) fn exec_integer_mask_vv<OpIMVV>(
        &mut self,
        vs1: u8,
        vs2: u8,
        vd: u8,
        enable_mask: bool,
    ) -> Result<(), Exception>
    where
        OpIMVV: VectorOpIntegerMaskVV,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (sew, lmul) = (vsew.into_byte_width(), vlmul.get_lmul());
        let vs1_data = self.vector_regfile.get_ref(lmul, 1, vs1)?.to_vec();
        let vs2_data = self.vector_regfile.get_ref(lmul, 1, vs2)?.to_vec();
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );
        let vs1_ref = VGFRef::new(&vs1_data, sew, lmul, 1);
        let vs2_ref = VGFRef::new(&vs2_data, sew, lmul, 1);
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut::<u8>(1, vd, 1)?,
            Vsew::E8.into_byte_width(),
            1,
            1,
        );
        OpIMVV::exec(&vs1_ref, &vs2_ref, &mut vd_ref, &mask)
    }

    #[inline]
    pub(in crate::isa::riscv) fn exec_integer_mask_vx<OpIMVX>(
        &mut self,
        x1: WordType,
        vs2: u8,
        vd: u8,
        enable_mask: bool,
    ) -> Result<(), Exception>
    where
        OpIMVX: VectorOpIntegerMaskVX,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (sew, lmul) = (vsew.into_byte_width(), vlmul.get_lmul());
        let vs2_data = self.vector_regfile.get_ref(lmul, 1, vs2)?.to_vec();
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );
        let vs2_ref = VGFRef::new(&vs2_data, sew, lmul, 1);
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut::<u8>(1, vd, 1)?,
            Vsew::E8.into_byte_width(),
            1,
            1,
        );
        OpIMVX::exec(x1, &vs2_ref, &mut vd_ref, &mask)
    }

    #[inline]
    pub(in crate::isa::riscv) fn exec_integer_mask_vvm<OpIMVVM>(
        &mut self,
        vs1: u8,
        vs2: u8,
        v0: u8,
        vd: u8,
        enable_mask: bool,
    ) -> Result<(), Exception>
    where
        OpIMVVM: VectorOpIntegerMaskVVM,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (sew, lmul) = (vsew.into_byte_width(), vlmul.get_lmul());
        let vs1_data = self.vector_regfile.get_ref(lmul, 1, vs1)?.to_vec();
        let vs2_data = self.vector_regfile.get_ref(lmul, 1, vs2)?.to_vec();
        let v0_data = self.vector_regfile.get_ref(1, 1, v0)?.to_vec();
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );
        let vs1_ref = VGFRef::new(&vs1_data, sew, lmul, 1);
        let vs2_ref = VGFRef::new(&vs2_data, sew, lmul, 1);
        let v0_ref = VGFRef::new(&v0_data, Vsew::E8.into_byte_width(), 1, 1);
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut::<u8>(1, vd, 1)?,
            Vsew::E8.into_byte_width(),
            1,
            1,
        );
        OpIMVVM::exec(&vs1_ref, &vs2_ref, &v0_ref, &mut vd_ref, &mask)
    }

    #[inline]
    pub(in crate::isa::riscv) fn exec_integer_mask_vxm<OpIMVXM>(
        &mut self,
        x1: WordType,
        vs2: u8,
        v0: u8,
        vd: u8,
        enable_mask: bool,
    ) -> Result<(), Exception>
    where
        OpIMVXM: VectorOpIntegerMaskVXM,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (sew, lmul) = (vsew.into_byte_width(), vlmul.get_lmul());
        let vs2_data = self.vector_regfile.get_ref(lmul, 1, vs2)?.to_vec();
        let v0_data = self.vector_regfile.get_ref(1, 1, v0)?.to_vec();
        let mask = VecOpMask::new(
            &self.vector_regfile,
            self.config.vl,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );
        let vs2_ref = VGFRef::new(&vs2_data, sew, lmul, 1);
        let v0_ref = VGFRef::new(&v0_data, Vsew::E8.into_byte_width(), 1, 1);
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut::<u8>(1, vd, 1)?,
            Vsew::E8.into_byte_width(),
            1,
            1,
        );
        OpIMVXM::exec(x1, &vs2_ref, &v0_ref, &mut vd_ref, &mask)
    }
}
