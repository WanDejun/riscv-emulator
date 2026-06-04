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
            types::{VGFRef, VGFRefMut},
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

                for (index, element) in vd.iter_mut().enumerate() {
                    match sew {
                        1 => {
                            let v1_value = vs1.get::<u8>(index);
                            let v2_value = vs2.get::<u8>(index);
                            mask.element_load(
                                element,
                                $exec_ty::<u8>::exec(v1_value, v2_value)?,
                                index,
                            );
                        }
                        2 => {
                            let v1_value = vs1.get::<u16>(index);
                            let v2_value = vs2.get::<u16>(index);
                            mask.element_load(
                                element,
                                $exec_ty::<u16>::exec(v1_value, v2_value)?,
                                index,
                            );
                        }
                        4 => {
                            let v1_value = vs1.get::<u32>(index);
                            let v2_value = vs2.get::<u32>(index);
                            mask.element_load(
                                element,
                                $exec_ty::<u32>::exec(v1_value, v2_value)?,
                                index,
                            );
                        }
                        8 => {
                            let v1_value = vs1.get::<u64>(index);
                            let v2_value = vs2.get::<u64>(index);
                            mask.element_load(
                                element,
                                $exec_ty::<u64>::exec(v1_value, v2_value)?,
                                index,
                            );
                        }
                        _ => unreachable!(),
                    }
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

                for (index, element) in vd.iter_mut().enumerate() {
                    match sew {
                        1 => {
                            let scalar = x1 as u8;
                            let v2_value = vs2.get::<u8>(index);
                            mask.element_load(
                                element,
                                $exec_ty::<u8>::exec(v2_value, scalar)?,
                                index,
                            );
                        }
                        2 => {
                            let scalar = x1 as u16;
                            let v2_value = vs2.get::<u16>(index);
                            mask.element_load(
                                element,
                                $exec_ty::<u16>::exec(v2_value, scalar)?,
                                index,
                            );
                        }
                        4 => {
                            let scalar = x1 as u32;
                            let v2_value = vs2.get::<u32>(index);
                            mask.element_load(
                                element,
                                $exec_ty::<u32>::exec(v2_value, scalar)?,
                                index,
                            );
                        }
                        8 => {
                            let scalar = x1 as u64;
                            let v2_value = vs2.get::<u64>(index);
                            mask.element_load(
                                element,
                                $exec_ty::<u64>::exec(v2_value, scalar)?,
                                index,
                            );
                        }
                        _ => unreachable!(),
                    }
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

                for (index, element) in vd.iter_mut().enumerate() {
                    match (src_sew, dst_sew) {
                        $(
                            ($src_sew, $dst_sew) => {
                                let value = vs2.get::<$src_ty>(index);
                                mask.element_load(
                                    element,
                                    $exec_ty::<$dst_ty, $src_ty>::exec(value)?,
                                    index,
                                );
                            }
                        )+
                        _ => unreachable!(),
                    }
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
    fn exec_integer_vv<'a, OpIVV>(
        &'a self,
        vs1: u8,
        vs2: u8,
        vd: u8,
        enable_mask: bool,
    ) -> Result<(), Exception>
    where
        OpIVV: VectorOpIntegerVV,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let vrf = &self.vector_regfile;
        let (sew, lmul) = (vsew.get_sew(), self.config.vlmul.get_lmul());
        let vs1_ref = VGFRef::new(vrf.get_ref(vlmul.get_lmul(), vs1, 1).unwrap(), sew, lmul, 1);
        let vs2_ref = VGFRef::new(vrf.get_ref(vlmul.get_lmul(), vs2, 1).unwrap(), sew, lmul, 1);
        let mut vd_ref =
            VGFRefMut::new(vrf.get_mut(vlmul.get_lmul(), vd, 1).unwrap(), sew, lmul, 1);
        let mask = VecOpMask::new(
            vrf,
            self.config.vl,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );
        OpIVV::exec(&vs1_ref, &vs2_ref, &mut vd_ref, &mask)
    }

    fn exec_integer_vx<'a, OpIVX>(
        &'a self,
        x1: WordType,
        vs2: u8,
        vd: u8,
        enable_mask: bool,
    ) -> Result<(), Exception>
    where
        OpIVX: VectorOpIntegerVX,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let vrf = &self.vector_regfile;
        let (sew, lmul) = (vsew.get_sew(), self.config.vlmul.get_lmul());
        let vs2_ref = VGFRef::new(vrf.get_ref(vlmul.get_lmul(), vs2, 1).unwrap(), sew, lmul, 1);
        let mut vd_ref =
            VGFRefMut::new(vrf.get_mut(vlmul.get_lmul(), vd, 1).unwrap(), sew, lmul, 1);
        let mask = VecOpMask::new(
            vrf,
            self.config.vl,
            enable_mask,
            self.config.mask_agnostic,
            self.config.tail_agnostic,
        );
        OpIVX::exec(x1, &vs2_ref, &mut vd_ref, &mask)
    }
}
