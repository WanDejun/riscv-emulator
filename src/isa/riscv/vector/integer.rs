use crate::{
    config::arch_config::WordType,
    isa::riscv::{
        instruction::exec_function::{
            ExecAdd, ExecAddu, ExecAnd, ExecOr, ExecSLL, ExecSRA, ExecSRL, ExecSext, ExecSub,
            ExecSubu, ExecTrait, ExecUnaryTrait, ExecXor, ExecZext,
        },
        trap::Exception,
        vector::{
            VecOpMask, Vector,
            types::{VGFRef, VGFRefMut, Vsew},
        },
    },
};

pub(super) trait VectorOpIntegerVV {
    fn exec(
        vs1: &VGFRef,
        vs2: &VGFRef,
        vd: &mut VGFRefMut,
        mask: &VecOpMask,
    ) -> Result<(), Exception>;
}

pub(super) trait VectorOpIntegerVX {
    fn exec(
        x1: WordType,
        vs2: &VGFRef,
        vd: &mut VGFRefMut,
        mask: &VecOpMask,
    ) -> Result<(), Exception>;
}

pub(super) trait VectorOpIntegerV {
    fn exec(vs2: &VGFRef, vd: &mut VGFRefMut, mask: &VecOpMask) -> Result<(), Exception>;
}

macro_rules! impl_vector_op_integer_vv_binary {
    ($op_ty:ty, $exec_ty:ident) => {
        impl VectorOpIntegerVV for $op_ty {
            fn exec(
                vs1: &VGFRef,
                vs2: &VGFRef,
                vd: &mut VGFRefMut,
                mask: &VecOpMask,
            ) -> Result<(), Exception> {
                assert!(vs1.sew == vs2.sew && vs1.sew == vd.sew);
                let sew = vd.sew;

                match sew {
                    1 => {
                        let vs1 = vs1.as_slice::<u8>();
                        let vs2 = vs2.as_slice::<u8>();
                        for (index, element) in vd.iter_mut().enumerate() {
                            mask.element_load(
                                element,
                                $exec_ty::<u8>::exec(vs1[index], vs2[index])?,
                                index,
                            );
                        }
                    }
                    2 => {
                        let vs1 = vs1.as_slice::<u16>();
                        let vs2 = vs2.as_slice::<u16>();
                        for (index, element) in vd.iter_mut().enumerate() {
                            mask.element_load(
                                element,
                                $exec_ty::<u16>::exec(vs1[index], vs2[index])?,
                                index,
                            );
                        }
                    }
                    4 => {
                        let vs1 = vs1.as_slice::<u32>();
                        let vs2 = vs2.as_slice::<u32>();
                        for (index, element) in vd.iter_mut().enumerate() {
                            mask.element_load(
                                element,
                                $exec_ty::<u32>::exec(vs1[index], vs2[index])?,
                                index,
                            );
                        }
                    }
                    8 => {
                        let vs1 = vs1.as_slice::<u64>();
                        let vs2 = vs2.as_slice::<u64>();
                        for (index, element) in vd.iter_mut().enumerate() {
                            mask.element_load(
                                element,
                                $exec_ty::<u64>::exec(vs1[index], vs2[index])?,
                                index,
                            );
                        }
                    }
                    _ => unreachable!(),
                }

                Ok(())
            }
        }
    };
}

macro_rules! impl_vector_op_integer_vx_binary {
    ($op_ty:ty, $exec_ty:ident) => {
        impl VectorOpIntegerVX for $op_ty {
            fn exec(
                x1: WordType,
                vs2: &VGFRef,
                vd: &mut VGFRefMut,
                mask: &VecOpMask,
            ) -> Result<(), Exception> {
                assert!(vs2.sew == vd.sew);
                let sew = vd.sew;

                match sew {
                    1 => {
                        let scalar = x1 as u8;
                        let vs2 = vs2.as_slice::<u8>();
                        for (index, element) in vd.iter_mut().enumerate() {
                            mask.element_load(
                                element,
                                $exec_ty::<u8>::exec(vs2[index], scalar)?,
                                index,
                            );
                        }
                    }
                    2 => {
                        let scalar = x1 as u16;
                        let vs2 = vs2.as_slice::<u16>();
                        for (index, element) in vd.iter_mut().enumerate() {
                            mask.element_load(
                                element,
                                $exec_ty::<u16>::exec(vs2[index], scalar)?,
                                index,
                            );
                        }
                    }
                    4 => {
                        let scalar = x1 as u32;
                        let vs2 = vs2.as_slice::<u32>();
                        for (index, element) in vd.iter_mut().enumerate() {
                            mask.element_load(
                                element,
                                $exec_ty::<u32>::exec(vs2[index], scalar)?,
                                index,
                            );
                        }
                    }
                    8 => {
                        let scalar = x1 as u64;
                        let vs2 = vs2.as_slice::<u64>();
                        for (index, element) in vd.iter_mut().enumerate() {
                            mask.element_load(
                                element,
                                $exec_ty::<u64>::exec(vs2[index], scalar)?,
                                index,
                            );
                        }
                    }
                    _ => unreachable!(),
                }

                Ok(())
            }
        }
    };
}

macro_rules! impl_vector_op_integer_v_unary_ext {
    ($op_ty:ty, $exec_ty:ident, $factor:literal, [$(($src_sew:literal, $dst_sew:literal, $src_ty:ty, $dst_ty:ty)),+ $(,)?]) => {
        impl VectorOpIntegerV for $op_ty {
            fn exec(vs2: &VGFRef, vd: &mut VGFRefMut, mask: &VecOpMask) -> Result<(), Exception> {
                let src_sew = vs2.sew;
                let dst_sew = vd.sew;
                assert!(dst_sew == src_sew * $factor);

                match (src_sew, dst_sew) {
                    $(
                        ($src_sew, $dst_sew) => {
                            let vs2 = vs2.as_slice::<$src_ty>();
                            for (index, element) in vd.iter_mut().enumerate() {
                                mask.element_load(
                                    element,
                                    $exec_ty::<$dst_ty, $src_ty>::exec(vs2[index])?,
                                    index,
                                );
                            }
                        }
                    )+
                    _ => unreachable!(),
                }

                Ok(())
            }
        }
    };
}

pub(super) struct VectorOpAdd;
pub(super) struct VectorOpAddu;
pub(super) struct VectorOpSub;
pub(super) struct VectorOpSubu;
pub(super) struct VectorOpAnd;
pub(super) struct VectorOpOr;
pub(super) struct VectorOpXor;
pub(super) struct VectorOpSll;
pub(super) struct VectorOpSrl;
pub(super) struct VectorOpSra;
pub(super) struct VectorOpZextVf2;
pub(super) struct VectorOpZextVf4;
pub(super) struct VectorOpZextVf8;
pub(super) struct VectorOpSextVf2;
pub(super) struct VectorOpSextVf4;
pub(super) struct VectorOpSextVf8;

impl_vector_op_integer_vv_binary!(VectorOpAdd, ExecAdd);
impl_vector_op_integer_vv_binary!(VectorOpAddu, ExecAddu);
impl_vector_op_integer_vv_binary!(VectorOpSub, ExecSub);
impl_vector_op_integer_vv_binary!(VectorOpSubu, ExecSubu);
impl_vector_op_integer_vv_binary!(VectorOpAnd, ExecAnd);
impl_vector_op_integer_vv_binary!(VectorOpOr, ExecOr);
impl_vector_op_integer_vv_binary!(VectorOpXor, ExecXor);
impl_vector_op_integer_vv_binary!(VectorOpSll, ExecSLL);
impl_vector_op_integer_vv_binary!(VectorOpSrl, ExecSRL);
impl_vector_op_integer_vv_binary!(VectorOpSra, ExecSRA);

impl_vector_op_integer_vx_binary!(VectorOpAdd, ExecAdd);
impl_vector_op_integer_vx_binary!(VectorOpAddu, ExecAddu);
impl_vector_op_integer_vx_binary!(VectorOpSub, ExecSub);
impl_vector_op_integer_vx_binary!(VectorOpSubu, ExecSubu);
impl_vector_op_integer_vx_binary!(VectorOpAnd, ExecAnd);
impl_vector_op_integer_vx_binary!(VectorOpOr, ExecOr);
impl_vector_op_integer_vx_binary!(VectorOpXor, ExecXor);
impl_vector_op_integer_vx_binary!(VectorOpSll, ExecSLL);
impl_vector_op_integer_vx_binary!(VectorOpSrl, ExecSRL);
impl_vector_op_integer_vx_binary!(VectorOpSra, ExecSRA);

impl_vector_op_integer_v_unary_ext!(
    VectorOpZextVf2,
    ExecZext,
    2,
    [(1, 2, u8, u16), (2, 4, u16, u32), (4, 8, u32, u64)]
);
impl_vector_op_integer_v_unary_ext!(
    VectorOpZextVf4,
    ExecZext,
    4,
    [(1, 4, u8, u32), (2, 8, u16, u64)]
);
impl_vector_op_integer_v_unary_ext!(VectorOpZextVf8, ExecZext, 8, [(1, 8, u8, u64)]);
impl_vector_op_integer_v_unary_ext!(
    VectorOpSextVf2,
    ExecSext,
    2,
    [(1, 2, u8, u16), (2, 4, u16, u32), (4, 8, u32, u64)]
);
impl_vector_op_integer_v_unary_ext!(
    VectorOpSextVf4,
    ExecSext,
    4,
    [(1, 4, u8, u32), (2, 8, u16, u64)]
);
impl_vector_op_integer_v_unary_ext!(VectorOpSextVf8, ExecSext, 8, [(1, 8, u8, u64)]);

impl Vector {
    fn exec_integer_vv<OpIVV>(
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
        let (sew, lmul) = (vsew.byte_width(), vlmul.get_lmul());
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

    fn exec_integer_vx<OpIVX>(
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
        let (sew, lmul) = (vsew.byte_width(), vlmul.get_lmul());
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

    fn exec_integer_v<OpIV>(
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
        let vs2_ref = VGFRef::new(&vs2_data, src_eew.byte_width(), lmul, 1);
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut(lmul, vd, 1)?,
            dst_eew.byte_width(),
            lmul,
            1,
        );
        OpIV::exec(&vs2_ref, &mut vd_ref, &mask)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::isa::riscv::vector::{
        VLEN_BYTE,
        tester::{VectorBuilder, VectorChecker},
        types::Vlmul,
    };

    fn run_test_integer_vv<OpIVV, F, G>(build: F, check: G)
    where
        OpIVV: VectorOpIntegerVV,
        F: FnOnce(VectorBuilder) -> VectorBuilder,
        G: FnOnce(VectorChecker) -> VectorChecker,
    {
        const VS1: u8 = 8;
        const VS2: u8 = 10;
        const VD: u8 = 12;

        let (mut vector, mut mmio) = build(VectorBuilder::new()).build();
        vector
            .exec_integer_vv::<OpIVV>(VS1, VS2, VD, false)
            .unwrap();
        check(VectorChecker::new(&mut vector, &mut mmio));
    }

    fn run_test_integer_vx<OpIVX, F, G>(x1: WordType, build: F, check: G)
    where
        OpIVX: VectorOpIntegerVX,
        F: FnOnce(VectorBuilder) -> VectorBuilder,
        G: FnOnce(VectorChecker) -> VectorChecker,
    {
        const VS2: u8 = 8;
        const VD: u8 = 10;

        let (mut vector, mut mmio) = build(VectorBuilder::new()).build();
        vector.exec_integer_vx::<OpIVX>(x1, VS2, VD, false).unwrap();
        check(VectorChecker::new(&mut vector, &mut mmio));
    }

    fn run_test_integer_v<OpIV, F, G>(src_eew: Vsew, dst_eew: Vsew, build: F, check: G)
    where
        OpIV: VectorOpIntegerV,
        F: FnOnce(VectorBuilder) -> VectorBuilder,
        G: FnOnce(VectorChecker) -> VectorChecker,
    {
        const VS2: u8 = 8;
        const VD: u8 = 10;

        let (mut vector, mut mmio) = build(VectorBuilder::new()).build();
        vector
            .exec_integer_v::<OpIV>(VS2, VD, src_eew, dst_eew, false)
            .unwrap();
        check(VectorChecker::new(&mut vector, &mut mmio));
    }

    #[test]
    fn test_vector_op_add_vv() {
        const LMUL: Vlmul = Vlmul::M2;
        const SEW: Vsew = Vsew::E32;
        const VS1: u8 = 8;
        const VS2: u8 = 10;
        const VD: u8 = 12;

        let elem_count = VLEN_BYTE * LMUL.get_lmul() as usize / size_of::<u32>();
        let vs1: Vec<u32> = (0..elem_count).map(|i| (i as u32) * 7 + 3).collect();
        let vs2: Vec<u32> = (0..elem_count)
            .map(|i| u32::MAX.wrapping_sub((i as u32) * 5))
            .collect();
        let expected: Vec<u32> = vs1
            .iter()
            .zip(vs2.iter())
            .map(|(lhs, rhs)| lhs.wrapping_add(*rhs))
            .collect();

        run_test_integer_vv::<VectorOpAdd, _, _>(
            |builder| {
                builder
                    .config(LMUL, SEW, false, false, elem_count as u16)
                    .reg(LMUL.get_lmul(), VS1, &vs1)
                    .reg(LMUL.get_lmul(), VS2, &vs2)
            },
            |checker| checker.reg(LMUL.get_lmul(), VD, &expected),
        );
    }

    #[test]
    fn test_vector_op_add_vx() {
        const LMUL: Vlmul = Vlmul::M2;
        const SEW: Vsew = Vsew::E16;
        const VS2: u8 = 8;
        const VD: u8 = 10;
        const SCALAR: WordType = 0x12f0;

        let elem_count = VLEN_BYTE * LMUL.get_lmul() as usize / size_of::<u16>();
        let vs2: Vec<u16> = (0..elem_count)
            .map(|i| u16::MAX.wrapping_sub((i as u16) * 17))
            .collect();
        let scalar = SCALAR as u16;
        let expected: Vec<u16> = vs2.iter().map(|value| value.wrapping_add(scalar)).collect();

        run_test_integer_vx::<VectorOpAdd, _, _>(
            SCALAR,
            |builder| {
                builder
                    .config(LMUL, SEW, false, false, elem_count as u16)
                    .reg(LMUL.get_lmul(), VS2, &vs2)
            },
            |checker| checker.reg(LMUL.get_lmul(), VD, &expected),
        );
    }

    #[test]
    fn test_vector_op_zext_vf2() {
        const LMUL: Vlmul = Vlmul::M1;
        const SRC_SEW: Vsew = Vsew::E8;
        const DST_SEW: Vsew = Vsew::E16;
        const VS2: u8 = 8;
        const VD: u8 = 10;

        let elem_count = VLEN_BYTE * LMUL.get_lmul() as usize / size_of::<u16>();
        let vs2: Vec<u8> = (0..VLEN_BYTE * LMUL.get_lmul() as usize)
            .map(|i| 0x80u8.wrapping_add(i as u8))
            .collect();
        let expected: Vec<u16> = vs2
            .iter()
            .take(elem_count)
            .map(|value| *value as u16)
            .collect();

        run_test_integer_v::<VectorOpZextVf2, _, _>(
            SRC_SEW,
            DST_SEW,
            |builder| {
                builder
                    .config(LMUL, DST_SEW, false, false, elem_count as u16)
                    .reg(LMUL.get_lmul(), VS2, &vs2)
            },
            |checker| checker.reg(LMUL.get_lmul(), VD, &expected),
        );
    }
}
