#[cfg(test)]
use crate::isa::riscv::vector::tester::TestOpParameter;
use crate::{
    config::arch_config::WordType,
    isa::riscv::{
        instruction::exec_function::{
            ExecAdd, ExecAddu, ExecAnd, ExecEqual, ExecOr, ExecSLL, ExecSRA, ExecSRL, ExecSext,
            ExecSub, ExecSubu, ExecTrait, ExecUnaryTrait, ExecUnsignedLess, ExecXor, ExecZext,
        },
        trap::Exception,
        vector::{
            VecOpMask, Vector,
            types::{VGFRef, VGFRefMut, Vsew},
        },
    },
    utils::{UnsignedInteger, as_signed_i128},
};

trait ExecTernaryTrait<OUT, IN = WordType> {
    fn exec(a: IN, b: IN, c: bool) -> OUT;
}

struct ExecAdc;
struct ExecSbc;
struct ExecAdcCarryNoCarry;
struct ExecSbcBorrowNoBorrow;
struct ExecAdcCarry;
struct ExecSbcBorrow;

impl<T> ExecTernaryTrait<Result<T, Exception>, T> for ExecAdc
where
    T: UnsignedInteger + Into<u128>,
{
    fn exec(a: T, b: T, c: bool) -> Result<T, Exception> {
        let a: u128 = a.into();
        let b: u128 = b.into();
        Ok(T::truncate_from(a + b + c as u128))
    }
}

impl<T> ExecTernaryTrait<Result<T, Exception>, T> for ExecSbc
where
    T: UnsignedInteger + Into<u128>,
{
    fn exec(a: T, b: T, c: bool) -> Result<T, Exception> {
        let a: u128 = a.into();
        let b: u128 = b.into();
        Ok(T::truncate_from(a.wrapping_sub(b + c as u128)))
    }
}

impl<T> ExecTernaryTrait<Result<bool, Exception>, T> for ExecAdcCarry
where
    T: UnsignedInteger + Into<u128>,
{
    fn exec(a: T, b: T, c: bool) -> Result<bool, Exception> {
        let a: u128 = a.into();
        let b: u128 = b.into();
        Ok((a + b + c as u128) >> T::BITS != 0)
    }
}

impl<T> ExecTrait<bool, T> for ExecAdcCarryNoCarry
where
    T: UnsignedInteger + Into<u128>,
{
    fn exec(a: T, b: T) -> bool {
        let a: u128 = a.into();
        let b: u128 = b.into();
        (a + b) >> T::BITS != 0
    }
}

impl<T> ExecTrait<bool, T> for ExecSbcBorrowNoBorrow
where
    T: UnsignedInteger + Into<u128>,
{
    fn exec(a: T, b: T) -> bool {
        let a: u128 = a.into();
        let b: u128 = b.into();
        a < b
    }
}

impl<T> ExecTernaryTrait<Result<bool, Exception>, T> for ExecSbcBorrow
where
    T: UnsignedInteger + Into<u128>,
{
    fn exec(a: T, b: T, c: bool) -> Result<bool, Exception> {
        let a: u128 = a.into();
        let b: u128 = b.into();
        Ok(a < b + c as u128)
    }
}

struct ExecUnsignedLessEqual;
struct ExecSignedLess;
struct ExecSignedLessEqual;
struct ExecUnsignedGreater;
struct ExecSignedGreater;
struct ExecUnsignedGreaterX;
struct ExecSignedGreaterX;

impl<T> ExecTrait<bool, T> for ExecUnsignedLessEqual
where
    T: UnsignedInteger,
{
    fn exec(a: T, b: T) -> bool {
        a <= b
    }
}

impl<T> ExecTrait<bool, T> for ExecSignedLess
where
    T: UnsignedInteger + Into<u128>,
{
    fn exec(a: T, b: T) -> bool {
        as_signed_i128(a) < as_signed_i128(b)
    }
}

impl<T> ExecTrait<bool, T> for ExecSignedLessEqual
where
    T: UnsignedInteger + Into<u128>,
{
    fn exec(a: T, b: T) -> bool {
        as_signed_i128(a) <= as_signed_i128(b)
    }
}

impl<T> ExecTrait<bool, T> for ExecUnsignedGreater
where
    T: UnsignedInteger,
{
    fn exec(a: T, b: T) -> bool {
        a > b
    }
}

impl<T> ExecTrait<bool, T> for ExecSignedGreater
where
    T: UnsignedInteger + Into<u128>,
{
    fn exec(a: T, b: T) -> bool {
        as_signed_i128(a) > as_signed_i128(b)
    }
}

impl<T> ExecTrait<bool, T> for ExecUnsignedGreaterX
where
    T: UnsignedInteger,
{
    fn exec(a: T, b: T) -> bool {
        b > a
    }
}

impl<T> ExecTrait<bool, T> for ExecSignedGreaterX
where
    T: UnsignedInteger + Into<u128>,
{
    fn exec(a: T, b: T) -> bool {
        as_signed_i128(b) > as_signed_i128(a)
    }
}

#[inline(always)]
fn read_mask_bit(mask: &[u8], index: usize) -> bool {
    (mask[index / 8] & (1 << (index % 8))) != 0
}

#[inline(always)]
fn write_mask_bit(mask: &mut [u8], index: usize, value: bool) {
    let byte = &mut mask[index / 8];
    let bit = 1 << (index % 8);
    if value {
        *byte |= bit;
    } else {
        *byte &= !bit;
    }
}

macro_rules! dispatch_integer_sew {
    ($sew:expr, |$ty:ident| $body:block) => {
        match $sew {
            1 => {
                type $ty = u8;
                $body
            }
            2 => {
                type $ty = u16;
                $body
            }
            4 => {
                type $ty = u32;
                $body
            }
            8 => {
                type $ty = u64;
                $body
            }
            _ => unreachable!(),
        }
    };
}

// OPIVV
pub(super) trait VectorOpIntegerVV {
    fn exec(
        vs1: &VGFRef,
        vs2: &VGFRef,
        vd: &mut VGFRefMut,
        mask: &VecOpMask,
    ) -> Result<(), Exception>;

    #[cfg(test)]
    fn test(vector: &mut Vector, param: TestOpParameter) -> Result<(), Exception>
    where
        Self: Sized,
    {
        vector.exec_integer_vv::<Self>(param.vs1(), param.vs2(), param.vd(), param.enable_mask())
    }
}

// OPIVX
pub(super) trait VectorOpIntegerVX {
    fn exec(
        x1: WordType,
        vs2: &VGFRef,
        vd: &mut VGFRefMut,
        mask: &VecOpMask,
    ) -> Result<(), Exception>;

    #[cfg(test)]
    fn test(vector: &mut Vector, param: TestOpParameter) -> Result<(), Exception>
    where
        Self: Sized,
    {
        vector.exec_integer_vx::<Self>(param.x1(), param.vs2(), param.vd(), param.enable_mask())
    }
}

// OPIV(Unary)
pub(super) trait VectorOpIntegerV {
    fn exec(vs2: &VGFRef, vd: &mut VGFRefMut, mask: &VecOpMask) -> Result<(), Exception>;

    #[cfg(test)]
    fn test(vector: &mut Vector, param: TestOpParameter) -> Result<(), Exception>
    where
        Self: Sized,
    {
        vector.exec_integer_v::<Self>(
            param.vs2(),
            param.vd(),
            param.src_eew(),
            param.dst_eew(),
            param.enable_mask(),
        )
    }
}

pub(super) trait VectorOpIntegerVVM {
    fn exec(vs1: &VGFRef, vs2: &VGFRef, v0: &VGFRef, vd: &mut VGFRefMut) -> Result<(), Exception>;

    #[cfg(test)]
    fn test(vector: &mut Vector, param: TestOpParameter) -> Result<(), Exception>
    where
        Self: Sized,
    {
        vector.exec_integer_vvm::<Self>(param.vs1(), param.vs2(), param.v0(), param.vd())
    }
}

pub(super) trait VectorOpIntegerVXM {
    fn exec(x1: WordType, vs2: &VGFRef, v0: &VGFRef, vd: &mut VGFRefMut) -> Result<(), Exception>;

    #[cfg(test)]
    fn test(vector: &mut Vector, param: TestOpParameter) -> Result<(), Exception>
    where
        Self: Sized,
    {
        vector.exec_integer_vxm::<Self>(param.x1(), param.vs2(), param.v0(), param.vd())
    }
}

pub(super) trait VectorOpIntegerMaskVV {
    fn exec(vs1: &VGFRef, vs2: &VGFRef, vd: &mut VGFRefMut) -> Result<(), Exception>;

    #[cfg(test)]
    fn test(vector: &mut Vector, param: TestOpParameter) -> Result<(), Exception>
    where
        Self: Sized,
    {
        vector.exec_integer_mask_vv::<Self>(param.vs1(), param.vs2(), param.vd())
    }
}

pub(super) trait VectorOpIntegerMaskVX {
    fn exec(x1: WordType, vs2: &VGFRef, vd: &mut VGFRefMut) -> Result<(), Exception>;

    #[cfg(test)]
    fn test(vector: &mut Vector, param: TestOpParameter) -> Result<(), Exception>
    where
        Self: Sized,
    {
        vector.exec_integer_mask_vx::<Self>(param.x1(), param.vs2(), param.vd())
    }
}

pub(super) trait VectorOpIntegerMaskVVM {
    fn exec(vs1: &VGFRef, vs2: &VGFRef, v0: &VGFRef, vd: &mut VGFRefMut) -> Result<(), Exception>;

    #[cfg(test)]
    fn test(vector: &mut Vector, param: TestOpParameter) -> Result<(), Exception>
    where
        Self: Sized,
    {
        vector.exec_integer_mask_vvm::<Self>(param.vs1(), param.vs2(), param.v0(), param.vd())
    }
}

pub(super) trait VectorOpIntegerMaskVXM {
    fn exec(x1: WordType, vs2: &VGFRef, v0: &VGFRef, vd: &mut VGFRefMut) -> Result<(), Exception>;

    #[cfg(test)]
    fn test(vector: &mut Vector, param: TestOpParameter) -> Result<(), Exception>
    where
        Self: Sized,
    {
        vector.exec_integer_mask_vxm::<Self>(param.x1(), param.vs2(), param.v0(), param.vd())
    }
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

                dispatch_integer_sew!(sew, |T| {
                    let vs1 = vs1.as_slice::<T>();
                    let vs2 = vs2.as_slice::<T>();
                    for (index, element) in vd.iter_mut().enumerate() {
                        mask.element_load(
                            element,
                            $exec_ty::<T>::exec(vs1[index], vs2[index])?,
                            index,
                        );
                    }
                });

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

                dispatch_integer_sew!(sew, |T| {
                    let scalar = x1 as T;
                    let vs2 = vs2.as_slice::<T>();
                    for (index, element) in vd.iter_mut().enumerate() {
                        mask.element_load(element, $exec_ty::<T>::exec(vs2[index], scalar)?, index);
                    }
                });

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

macro_rules! impl_vector_op_integer_vvm_binary {
    ($op_ty:ty, $exec_ty:ident) => {
        impl VectorOpIntegerVVM for $op_ty {
            fn exec(
                vs1: &VGFRef,
                vs2: &VGFRef,
                v0: &VGFRef,
                vd: &mut VGFRefMut,
            ) -> Result<(), Exception> {
                assert!(vs1.sew == vs2.sew && vs1.sew == vd.sew);
                let carry = v0.as_slice::<u8>();
                dispatch_integer_sew!(vd.sew, |T| {
                    let vs1 = vs1.as_slice::<T>();
                    let vs2 = vs2.as_slice::<T>();
                    for (index, element) in vd.iter_mut().enumerate() {
                        element.set($exec_ty::exec(
                            vs2[index],
                            vs1[index],
                            read_mask_bit(carry, index),
                        )?);
                    }
                });
                Ok(())
            }
        }
    };
}

macro_rules! impl_vector_op_integer_vxm_binary {
    ($op_ty:ty, $exec_ty:ident) => {
        impl VectorOpIntegerVXM for $op_ty {
            fn exec(
                x1: WordType,
                vs2: &VGFRef,
                v0: &VGFRef,
                vd: &mut VGFRefMut,
            ) -> Result<(), Exception> {
                assert!(vs2.sew == vd.sew);
                let carry = v0.as_slice::<u8>();
                dispatch_integer_sew!(vd.sew, |T| {
                    let scalar = x1 as T;
                    let vs2 = vs2.as_slice::<T>();
                    for (index, element) in vd.iter_mut().enumerate() {
                        element.set($exec_ty::exec(
                            vs2[index],
                            scalar,
                            read_mask_bit(carry, index),
                        )?);
                    }
                });
                Ok(())
            }
        }
    };
}

macro_rules! impl_vector_op_integer_mask_vv_binary {
    ($op_ty:ty, $exec_ty:ident) => {
        impl VectorOpIntegerMaskVV for $op_ty {
            fn exec(vs1: &VGFRef, vs2: &VGFRef, vd: &mut VGFRefMut) -> Result<(), Exception> {
                assert!(vs1.sew == vs2.sew);
                let mask = vd.as_mut_slice::<u8>();
                dispatch_integer_sew!(vs2.sew, |T| {
                    let vs1 = vs1.as_slice::<T>();
                    let vs2 = vs2.as_slice::<T>();
                    for index in 0..vs2.len() {
                        write_mask_bit(mask, index, $exec_ty::exec(vs2[index], vs1[index]));
                    }
                });
                Ok(())
            }
        }
    };
}

macro_rules! impl_vector_op_integer_mask_vx_binary {
    ($op_ty:ty, $exec_ty:ident) => {
        impl VectorOpIntegerMaskVX for $op_ty {
            fn exec(x1: WordType, vs2: &VGFRef, vd: &mut VGFRefMut) -> Result<(), Exception> {
                let mask = vd.as_mut_slice::<u8>();
                dispatch_integer_sew!(vs2.sew, |T| {
                    let scalar = x1 as T;
                    let vs2 = vs2.as_slice::<T>();
                    for index in 0..vs2.len() {
                        write_mask_bit(mask, index, $exec_ty::exec(vs2[index], scalar));
                    }
                });
                Ok(())
            }
        }
    };
}

macro_rules! impl_vector_op_integer_mask_vvm_binary {
    ($op_ty:ty, $exec_ty:ident) => {
        impl VectorOpIntegerMaskVVM for $op_ty {
            fn exec(
                vs1: &VGFRef,
                vs2: &VGFRef,
                v0: &VGFRef,
                vd: &mut VGFRefMut,
            ) -> Result<(), Exception> {
                assert!(vs1.sew == vs2.sew);
                let carry = v0.as_slice::<u8>();
                let mask = vd.as_mut_slice::<u8>();
                dispatch_integer_sew!(vs2.sew, |T| {
                    let vs1 = vs1.as_slice::<T>();
                    let vs2 = vs2.as_slice::<T>();
                    for index in 0..vs2.len() {
                        write_mask_bit(
                            mask,
                            index,
                            $exec_ty::exec(vs2[index], vs1[index], read_mask_bit(carry, index))?,
                        );
                    }
                });
                Ok(())
            }
        }
    };
}

macro_rules! impl_vector_op_integer_mask_vxm_binary {
    ($op_ty:ty, $exec_ty:ident) => {
        impl VectorOpIntegerMaskVXM for $op_ty {
            fn exec(
                x1: WordType,
                vs2: &VGFRef,
                v0: &VGFRef,
                vd: &mut VGFRefMut,
            ) -> Result<(), Exception> {
                let carry = v0.as_slice::<u8>();
                let mask = vd.as_mut_slice::<u8>();
                dispatch_integer_sew!(vs2.sew, |T| {
                    let scalar = x1 as T;
                    let vs2 = vs2.as_slice::<T>();
                    for index in 0..vs2.len() {
                        write_mask_bit(
                            mask,
                            index,
                            $exec_ty::exec(vs2[index], scalar, read_mask_bit(carry, index))?,
                        );
                    }
                });
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
pub(super) struct VectorOpAdc;
pub(super) struct VectorOpSbc;
pub(super) struct VectorOpMadc;
pub(super) struct VectorOpMsbc;
pub(super) struct VectorOpMseq;
pub(super) struct VectorOpMsltu;
pub(super) struct VectorOpMslt;
pub(super) struct VectorOpMsleu;
pub(super) struct VectorOpMsle;
pub(super) struct VectorOpMsgtu;
pub(super) struct VectorOpMsgt;
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

impl_vector_op_integer_vvm_binary!(VectorOpAdc, ExecAdc);
impl_vector_op_integer_vxm_binary!(VectorOpAdc, ExecAdc);
impl_vector_op_integer_vvm_binary!(VectorOpSbc, ExecSbc);
impl_vector_op_integer_vxm_binary!(VectorOpSbc, ExecSbc);

impl_vector_op_integer_mask_vv_binary!(VectorOpMadc, ExecAdcCarryNoCarry);
impl_vector_op_integer_mask_vx_binary!(VectorOpMadc, ExecAdcCarryNoCarry);
impl_vector_op_integer_mask_vvm_binary!(VectorOpMadc, ExecAdcCarry);
impl_vector_op_integer_mask_vxm_binary!(VectorOpMadc, ExecAdcCarry);
impl_vector_op_integer_mask_vv_binary!(VectorOpMsbc, ExecSbcBorrowNoBorrow);
impl_vector_op_integer_mask_vx_binary!(VectorOpMsbc, ExecSbcBorrowNoBorrow);
impl_vector_op_integer_mask_vvm_binary!(VectorOpMsbc, ExecSbcBorrow);
impl_vector_op_integer_mask_vxm_binary!(VectorOpMsbc, ExecSbcBorrow);

impl_vector_op_integer_mask_vv_binary!(VectorOpMseq, ExecEqual);
impl_vector_op_integer_mask_vx_binary!(VectorOpMseq, ExecEqual);
impl_vector_op_integer_mask_vv_binary!(VectorOpMsltu, ExecUnsignedLess);
impl_vector_op_integer_mask_vx_binary!(VectorOpMsltu, ExecUnsignedLess);
impl_vector_op_integer_mask_vv_binary!(VectorOpMslt, ExecSignedLess);
impl_vector_op_integer_mask_vx_binary!(VectorOpMslt, ExecSignedLess);
impl_vector_op_integer_mask_vv_binary!(VectorOpMsleu, ExecUnsignedLessEqual);
impl_vector_op_integer_mask_vx_binary!(VectorOpMsleu, ExecUnsignedLessEqual);
impl_vector_op_integer_mask_vv_binary!(VectorOpMsle, ExecSignedLessEqual);
impl_vector_op_integer_mask_vx_binary!(VectorOpMsle, ExecSignedLessEqual);
impl_vector_op_integer_mask_vx_binary!(VectorOpMsgtu, ExecUnsignedGreaterX);
impl_vector_op_integer_mask_vx_binary!(VectorOpMsgt, ExecSignedGreaterX);

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

    #[cfg(test)]
    fn exec_integer_vvm<OpIVVM>(
        &mut self,
        vs1: u8,
        vs2: u8,
        v0: u8,
        vd: u8,
    ) -> Result<(), Exception>
    where
        OpIVVM: VectorOpIntegerVVM,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (sew, lmul) = (vsew.byte_width(), vlmul.get_lmul());
        let vs1_data = self.vector_regfile.get_ref(lmul, 1, vs1)?.to_vec();
        let vs2_data = self.vector_regfile.get_ref(lmul, 1, vs2)?.to_vec();
        let v0_data = self.vector_regfile.get_ref(1, 1, v0)?.to_vec();
        let vs1_ref = VGFRef::new(&vs1_data, sew, lmul, 1);
        let vs2_ref = VGFRef::new(&vs2_data, sew, lmul, 1);
        let v0_ref = VGFRef::new(&v0_data, Vsew::E8.byte_width(), 1, 1);
        let mut vd_ref = VGFRefMut::new(self.vector_regfile.get_mut(lmul, vd, 1)?, sew, lmul, 1);
        OpIVVM::exec(&vs1_ref, &vs2_ref, &v0_ref, &mut vd_ref)
    }

    #[cfg(test)]
    fn exec_integer_vxm<OpIVXM>(
        &mut self,
        x1: WordType,
        vs2: u8,
        v0: u8,
        vd: u8,
    ) -> Result<(), Exception>
    where
        OpIVXM: VectorOpIntegerVXM,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (sew, lmul) = (vsew.byte_width(), vlmul.get_lmul());
        let vs2_data = self.vector_regfile.get_ref(lmul, 1, vs2)?.to_vec();
        let v0_data = self.vector_regfile.get_ref(1, 1, v0)?.to_vec();
        let vs2_ref = VGFRef::new(&vs2_data, sew, lmul, 1);
        let v0_ref = VGFRef::new(&v0_data, Vsew::E8.byte_width(), 1, 1);
        let mut vd_ref = VGFRefMut::new(self.vector_regfile.get_mut(lmul, vd, 1)?, sew, lmul, 1);
        OpIVXM::exec(x1, &vs2_ref, &v0_ref, &mut vd_ref)
    }

    #[cfg(test)]
    fn exec_integer_mask_vv<OpIMVV>(&mut self, vs1: u8, vs2: u8, vd: u8) -> Result<(), Exception>
    where
        OpIMVV: VectorOpIntegerMaskVV,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (sew, lmul) = (vsew.byte_width(), vlmul.get_lmul());
        let vs1_data = self.vector_regfile.get_ref(lmul, 1, vs1)?.to_vec();
        let vs2_data = self.vector_regfile.get_ref(lmul, 1, vs2)?.to_vec();
        let vs1_ref = VGFRef::new(&vs1_data, sew, lmul, 1);
        let vs2_ref = VGFRef::new(&vs2_data, sew, lmul, 1);
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut::<u8>(1, vd, 1)?,
            Vsew::E8.byte_width(),
            1,
            1,
        );
        OpIMVV::exec(&vs1_ref, &vs2_ref, &mut vd_ref)
    }

    #[cfg(test)]
    fn exec_integer_mask_vx<OpIMVX>(
        &mut self,
        x1: WordType,
        vs2: u8,
        vd: u8,
    ) -> Result<(), Exception>
    where
        OpIMVX: VectorOpIntegerMaskVX,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (sew, lmul) = (vsew.byte_width(), vlmul.get_lmul());
        let vs2_data = self.vector_regfile.get_ref(lmul, 1, vs2)?.to_vec();
        let vs2_ref = VGFRef::new(&vs2_data, sew, lmul, 1);
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut::<u8>(1, vd, 1)?,
            Vsew::E8.byte_width(),
            1,
            1,
        );
        OpIMVX::exec(x1, &vs2_ref, &mut vd_ref)
    }

    #[cfg(test)]
    fn exec_integer_mask_vvm<OpIMVVM>(
        &mut self,
        vs1: u8,
        vs2: u8,
        v0: u8,
        vd: u8,
    ) -> Result<(), Exception>
    where
        OpIMVVM: VectorOpIntegerMaskVVM,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (sew, lmul) = (vsew.byte_width(), vlmul.get_lmul());
        let vs1_data = self.vector_regfile.get_ref(lmul, 1, vs1)?.to_vec();
        let vs2_data = self.vector_regfile.get_ref(lmul, 1, vs2)?.to_vec();
        let v0_data = self.vector_regfile.get_ref(1, 1, v0)?.to_vec();
        let vs1_ref = VGFRef::new(&vs1_data, sew, lmul, 1);
        let vs2_ref = VGFRef::new(&vs2_data, sew, lmul, 1);
        let v0_ref = VGFRef::new(&v0_data, Vsew::E8.byte_width(), 1, 1);
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut::<u8>(1, vd, 1)?,
            Vsew::E8.byte_width(),
            1,
            1,
        );
        OpIMVVM::exec(&vs1_ref, &vs2_ref, &v0_ref, &mut vd_ref)
    }

    #[cfg(test)]
    fn exec_integer_mask_vxm<OpIMVXM>(
        &mut self,
        x1: WordType,
        vs2: u8,
        v0: u8,
        vd: u8,
    ) -> Result<(), Exception>
    where
        OpIMVXM: VectorOpIntegerMaskVXM,
    {
        let (vlmul, vsew) = (self.config.vlmul, self.config.vsew);
        let (sew, lmul) = (vsew.byte_width(), vlmul.get_lmul());
        let vs2_data = self.vector_regfile.get_ref(lmul, 1, vs2)?.to_vec();
        let v0_data = self.vector_regfile.get_ref(1, 1, v0)?.to_vec();
        let vs2_ref = VGFRef::new(&vs2_data, sew, lmul, 1);
        let v0_ref = VGFRef::new(&v0_data, Vsew::E8.byte_width(), 1, 1);
        let mut vd_ref = VGFRefMut::new(
            self.vector_regfile.get_mut::<u8>(1, vd, 1)?,
            Vsew::E8.byte_width(),
            1,
            1,
        );
        OpIMVXM::exec(x1, &vs2_ref, &v0_ref, &mut vd_ref)
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

    fn run_test_integer_vv<OpIVV, F, G>(param: TestOpParameter, build: F, check: G)
    where
        OpIVV: VectorOpIntegerVV,
        F: FnOnce(VectorBuilder) -> VectorBuilder,
        G: FnOnce(VectorChecker) -> VectorChecker,
    {
        let (mut vector, mut mmio) = build(VectorBuilder::new()).build();
        OpIVV::test(&mut vector, param).unwrap();
        check(VectorChecker::new(&mut vector, &mut mmio));
    }

    fn run_test_integer_vx<OpIVX, F, G>(param: TestOpParameter, build: F, check: G)
    where
        OpIVX: VectorOpIntegerVX,
        F: FnOnce(VectorBuilder) -> VectorBuilder,
        G: FnOnce(VectorChecker) -> VectorChecker,
    {
        let (mut vector, mut mmio) = build(VectorBuilder::new()).build();
        OpIVX::test(&mut vector, param).unwrap();
        check(VectorChecker::new(&mut vector, &mut mmio));
    }

    fn run_test_integer_v<OpIV, F, G>(param: TestOpParameter, build: F, check: G)
    where
        OpIV: VectorOpIntegerV,
        F: FnOnce(VectorBuilder) -> VectorBuilder,
        G: FnOnce(VectorChecker) -> VectorChecker,
    {
        let (mut vector, mut mmio) = build(VectorBuilder::new()).build();
        OpIV::test(&mut vector, param).unwrap();
        check(VectorChecker::new(&mut vector, &mut mmio));
    }

    fn run_test_integer_vvm<OpIVVM, F, G>(param: TestOpParameter, build: F, check: G)
    where
        OpIVVM: VectorOpIntegerVVM,
        F: FnOnce(VectorBuilder) -> VectorBuilder,
        G: FnOnce(VectorChecker) -> VectorChecker,
    {
        let (mut vector, mut mmio) = build(VectorBuilder::new()).build();
        OpIVVM::test(&mut vector, param).unwrap();
        check(VectorChecker::new(&mut vector, &mut mmio));
    }

    fn run_test_integer_mask_vv<OpIMVV, F, G>(param: TestOpParameter, build: F, check: G)
    where
        OpIMVV: VectorOpIntegerMaskVV,
        F: FnOnce(VectorBuilder) -> VectorBuilder,
        G: FnOnce(VectorChecker) -> VectorChecker,
    {
        let (mut vector, mut mmio) = build(VectorBuilder::new()).build();
        OpIMVV::test(&mut vector, param).unwrap();
        check(VectorChecker::new(&mut vector, &mut mmio));
    }

    fn run_test_integer_mask_vx<OpIMVX, F, G>(param: TestOpParameter, build: F, check: G)
    where
        OpIMVX: VectorOpIntegerMaskVX,
        F: FnOnce(VectorBuilder) -> VectorBuilder,
        G: FnOnce(VectorChecker) -> VectorChecker,
    {
        let (mut vector, mut mmio) = build(VectorBuilder::new()).build();
        OpIMVX::test(&mut vector, param).unwrap();
        check(VectorChecker::new(&mut vector, &mut mmio));
    }

    fn run_test_integer_mask_vvm<OpIMVVM, F, G>(param: TestOpParameter, build: F, check: G)
    where
        OpIMVVM: VectorOpIntegerMaskVVM,
        F: FnOnce(VectorBuilder) -> VectorBuilder,
        G: FnOnce(VectorChecker) -> VectorChecker,
    {
        let (mut vector, mut mmio) = build(VectorBuilder::new()).build();
        OpIMVVM::test(&mut vector, param).unwrap();
        check(VectorChecker::new(&mut vector, &mut mmio));
    }

    fn mask_from_bits<I>(bits: I) -> Vec<u8>
    where
        I: IntoIterator<Item = bool>,
    {
        let mut mask = vec![0; VLEN_BYTE];
        for (index, bit) in bits.into_iter().enumerate() {
            write_mask_bit(&mut mask, index, bit);
        }
        mask
    }

    fn mask_bit(mask: &[u8], index: usize) -> bool {
        read_mask_bit(mask, index)
    }

    fn run_vv_binary_u32<OpIVV>(vs1: &[u32], vs2: &[u32], expected: &[u32])
    where
        OpIVV: VectorOpIntegerVV,
    {
        const LMUL: Vlmul = Vlmul::M1;
        const SEW: Vsew = Vsew::E32;
        let param = TestOpParameter::new_vv(8, 16, 24);

        run_test_integer_vv::<OpIVV, _, _>(
            param,
            |builder| {
                builder
                    .config(LMUL, SEW, false, false, expected.len() as u16)
                    .reg(LMUL.get_lmul(), param.vs1(), vs1)
                    .reg(LMUL.get_lmul(), param.vs2(), vs2)
            },
            |checker| checker.reg(LMUL.get_lmul(), param.vd(), expected),
        );
    }

    fn run_vx_binary_u32<OpIVX>(scalar: WordType, vs2: &[u32], expected: &[u32])
    where
        OpIVX: VectorOpIntegerVX,
    {
        const LMUL: Vlmul = Vlmul::M1;
        const SEW: Vsew = Vsew::E32;
        let param = TestOpParameter::new_vx(scalar, 8, 24);

        run_test_integer_vx::<OpIVX, _, _>(
            param,
            |builder| {
                builder
                    .config(LMUL, SEW, false, false, expected.len() as u16)
                    .reg(LMUL.get_lmul(), param.vs2(), vs2)
            },
            |checker| checker.reg(LMUL.get_lmul(), param.vd(), expected),
        );
    }

    fn run_vvm_binary_u8<OpIVVM>(vs1: &[u8], vs2: &[u8], carry: &[u8], expected: &[u8])
    where
        OpIVVM: VectorOpIntegerVVM,
    {
        const LMUL: Vlmul = Vlmul::M1;
        const SEW: Vsew = Vsew::E8;
        let param = TestOpParameter::new_vvm(8, 16, 24);

        run_test_integer_vvm::<OpIVVM, _, _>(
            param,
            |builder| {
                builder
                    .config(LMUL, SEW, false, false, expected.len() as u16)
                    .reg(1, param.v0(), carry)
                    .reg(LMUL.get_lmul(), param.vs1(), vs1)
                    .reg(LMUL.get_lmul(), param.vs2(), vs2)
            },
            |checker| checker.reg(LMUL.get_lmul(), param.vd(), expected),
        );
    }

    fn run_mask_vv_u8<OpIMVV>(vs1: &[u8], vs2: &[u8], expected: &[u8])
    where
        OpIMVV: VectorOpIntegerMaskVV,
    {
        const LMUL: Vlmul = Vlmul::M1;
        const SEW: Vsew = Vsew::E8;
        let param = TestOpParameter::new_mask_vv(8, 16, 24);

        run_test_integer_mask_vv::<OpIMVV, _, _>(
            param,
            |builder| {
                builder
                    .config(LMUL, SEW, false, false, vs2.len() as u16)
                    .reg(LMUL.get_lmul(), param.vs1(), vs1)
                    .reg(LMUL.get_lmul(), param.vs2(), vs2)
            },
            |checker| checker.reg(1, param.vd(), expected),
        );
    }

    fn run_mask_vx_u8<OpIMVX>(scalar: WordType, vs2: &[u8], expected: &[u8])
    where
        OpIMVX: VectorOpIntegerMaskVX,
    {
        const LMUL: Vlmul = Vlmul::M1;
        const SEW: Vsew = Vsew::E8;
        let param = TestOpParameter::new_mask_vx(scalar, 8, 24);

        run_test_integer_mask_vx::<OpIMVX, _, _>(
            param,
            |builder| {
                builder
                    .config(LMUL, SEW, false, false, vs2.len() as u16)
                    .reg(LMUL.get_lmul(), param.vs2(), vs2)
            },
            |checker| checker.reg(1, param.vd(), expected),
        );
    }

    fn run_mask_vvm_u8<OpIMVVM>(vs1: &[u8], vs2: &[u8], carry: &[u8], expected: &[u8])
    where
        OpIMVVM: VectorOpIntegerMaskVVM,
    {
        const LMUL: Vlmul = Vlmul::M1;
        const SEW: Vsew = Vsew::E8;
        let param = TestOpParameter::new_mask_vvm(8, 16, 24);

        run_test_integer_mask_vvm::<OpIMVVM, _, _>(
            param,
            |builder| {
                builder
                    .config(LMUL, SEW, false, false, vs2.len() as u16)
                    .reg(1, param.v0(), carry)
                    .reg(LMUL.get_lmul(), param.vs1(), vs1)
                    .reg(LMUL.get_lmul(), param.vs2(), vs2)
            },
            |checker| checker.reg(1, param.vd(), expected),
        );
    }

    #[test]
    fn test_vector_op_add_vv() {
        const LMUL: Vlmul = Vlmul::M2;
        const SEW: Vsew = Vsew::E32;
        let param = TestOpParameter::new_vv(8, 16, 24);

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
            param,
            |builder| {
                builder
                    .config(LMUL, SEW, false, false, elem_count as u16)
                    .reg(LMUL.get_lmul(), param.vs1(), &vs1)
                    .reg(LMUL.get_lmul(), param.vs2(), &vs2)
            },
            |checker| checker.reg(LMUL.get_lmul(), param.vd(), &expected),
        );
    }

    #[test]
    fn test_vector_op_integer_vv_binary() {
        let elem_count = VLEN_BYTE / size_of::<u32>();
        let vs1: Vec<u32> = (0..elem_count)
            .map(|i| 0x8000_0011u32.wrapping_add((i as u32) * 0x1020_304))
            .collect();
        let vs2: Vec<u32> = (0..elem_count)
            .map(|i| 0xfedc_ba98u32.wrapping_sub((i as u32) * 0x0101_0101))
            .collect();

        let expected: Vec<u32> = vs1
            .iter()
            .zip(vs2.iter())
            .map(|(lhs, rhs)| lhs.wrapping_add(*rhs))
            .collect();
        run_vv_binary_u32::<VectorOpAddu>(&vs1, &vs2, &expected);

        let expected: Vec<u32> = vs1
            .iter()
            .zip(vs2.iter())
            .map(|(lhs, rhs)| lhs.wrapping_sub(*rhs))
            .collect();
        run_vv_binary_u32::<VectorOpSub>(&vs1, &vs2, &expected);
        run_vv_binary_u32::<VectorOpSubu>(&vs1, &vs2, &expected);

        let expected: Vec<u32> = vs1
            .iter()
            .zip(vs2.iter())
            .map(|(lhs, rhs)| lhs & rhs)
            .collect();
        run_vv_binary_u32::<VectorOpAnd>(&vs1, &vs2, &expected);

        let expected: Vec<u32> = vs1
            .iter()
            .zip(vs2.iter())
            .map(|(lhs, rhs)| lhs.wrapping_shl(rhs & 31))
            .collect();
        run_vv_binary_u32::<VectorOpSll>(&vs1, &vs2, &expected);

        let expected: Vec<u32> = vs1
            .iter()
            .zip(vs2.iter())
            .map(|(lhs, rhs)| lhs.wrapping_shr(rhs & 31))
            .collect();
        run_vv_binary_u32::<VectorOpSrl>(&vs1, &vs2, &expected);

        let expected: Vec<u32> = vs1
            .iter()
            .zip(vs2.iter())
            .map(|(lhs, rhs)| (as_signed_i128(*lhs) >> (rhs & 31)) as u32)
            .collect();
        run_vv_binary_u32::<VectorOpSra>(&vs1, &vs2, &expected);
    }

    #[test]
    fn test_vector_op_add_vx() {
        const LMUL: Vlmul = Vlmul::M2;
        const SEW: Vsew = Vsew::E16;
        const SCALAR: WordType = 0x12f0;
        let param = TestOpParameter::new_vx(SCALAR, 8, 10);

        let elem_count = VLEN_BYTE * LMUL.get_lmul() as usize / size_of::<u16>();
        let vs2: Vec<u16> = (0..elem_count)
            .map(|i| u16::MAX.wrapping_sub((i as u16) * 17))
            .collect();
        let scalar = param.x1() as u16;
        let expected: Vec<u16> = vs2.iter().map(|value| value.wrapping_add(scalar)).collect();

        run_test_integer_vx::<VectorOpAdd, _, _>(
            param,
            |builder| {
                builder
                    .config(LMUL, SEW, false, false, elem_count as u16)
                    .reg(LMUL.get_lmul(), param.vs2(), &vs2)
            },
            |checker| checker.reg(LMUL.get_lmul(), param.vd(), &expected),
        );
    }

    #[test]
    fn test_vector_op_integer_vx_binary() {
        let elem_count = VLEN_BYTE / size_of::<u32>();
        let vs2: Vec<u32> = (0..elem_count)
            .map(|i| 0x8000_00f0u32.wrapping_add((i as u32) * 0x0110_0101))
            .collect();
        let scalar = 0x1234_0005u64;
        let scalar_u32 = scalar as u32;

        let expected: Vec<u32> = vs2
            .iter()
            .map(|value| value.wrapping_add(scalar_u32))
            .collect();
        run_vx_binary_u32::<VectorOpAddu>(scalar, &vs2, &expected);

        let expected: Vec<u32> = vs2
            .iter()
            .map(|value| value.wrapping_sub(scalar_u32))
            .collect();
        run_vx_binary_u32::<VectorOpSub>(scalar, &vs2, &expected);
        run_vx_binary_u32::<VectorOpSubu>(scalar, &vs2, &expected);

        let expected: Vec<u32> = vs2.iter().map(|value| value & scalar_u32).collect();
        run_vx_binary_u32::<VectorOpAnd>(scalar, &vs2, &expected);

        let shift = scalar_u32 & 31;
        let expected: Vec<u32> = vs2.iter().map(|value| value.wrapping_shl(shift)).collect();
        run_vx_binary_u32::<VectorOpSll>(scalar, &vs2, &expected);

        let expected: Vec<u32> = vs2.iter().map(|value| value.wrapping_shr(shift)).collect();
        run_vx_binary_u32::<VectorOpSrl>(scalar, &vs2, &expected);

        let expected: Vec<u32> = vs2
            .iter()
            .map(|value| (as_signed_i128(*value) >> shift) as u32)
            .collect();
        run_vx_binary_u32::<VectorOpSra>(scalar, &vs2, &expected);
    }

    #[test]
    fn test_vector_op_adc_sbc_vvm() {
        let elem_count = VLEN_BYTE;
        let vs1: Vec<u8> = (0..elem_count)
            .map(|i| 0x11u8.wrapping_add((i as u8).wrapping_mul(13)))
            .collect();
        let vs2: Vec<u8> = (0..elem_count)
            .map(|i| 0xf0u8.wrapping_sub((i as u8).wrapping_mul(7)))
            .collect();
        let carry = mask_from_bits((0..elem_count).map(|i| i % 3 == 1));

        let expected: Vec<u8> = vs1
            .iter()
            .zip(vs2.iter())
            .enumerate()
            .map(|(index, (vs1, vs2))| {
                vs2.wrapping_add(*vs1)
                    .wrapping_add(mask_bit(&carry, index) as u8)
            })
            .collect();
        run_vvm_binary_u8::<VectorOpAdc>(&vs1, &vs2, &carry, &expected);

        let expected: Vec<u8> = vs1
            .iter()
            .zip(vs2.iter())
            .enumerate()
            .map(|(index, (vs1, vs2))| {
                vs2.wrapping_sub(*vs1)
                    .wrapping_sub(mask_bit(&carry, index) as u8)
            })
            .collect();
        run_vvm_binary_u8::<VectorOpSbc>(&vs1, &vs2, &carry, &expected);
    }

    #[test]
    fn test_vector_op_mask_vv() {
        let elem_count = VLEN_BYTE;
        let vs1: Vec<u8> = (0..elem_count)
            .map(|i| [3, 7, 7, 10, 0xf0, 0x80, 0xff, 1][i % 8])
            .collect();
        let vs2: Vec<u8> = (0..elem_count)
            .map(|i| [3, 8, 6, 10, 0x10, 0x7f, 0xfe, 2][i % 8])
            .collect();

        let expected = mask_from_bits(vs1.iter().zip(vs2.iter()).map(|(vs1, vs2)| vs2 == vs1));
        run_mask_vv_u8::<VectorOpMseq>(&vs1, &vs2, &expected);

        let expected = mask_from_bits(vs1.iter().zip(vs2.iter()).map(|(vs1, vs2)| vs2 < vs1));
        run_mask_vv_u8::<VectorOpMsltu>(&vs1, &vs2, &expected);
    }

    #[test]
    fn test_vector_op_mask_vx() {
        let elem_count = VLEN_BYTE;
        let vs2: Vec<u8> = (0..elem_count)
            .map(|i| [0x10, 0x40, 0x80, 0xff, 0x7f, 0x01, 0x40, 0x41][i % 8])
            .collect();

        let scalar = 0x40;
        let expected = mask_from_bits(vs2.iter().map(|value| *value == scalar as u8));
        run_mask_vx_u8::<VectorOpMseq>(scalar, &vs2, &expected);

        let expected = mask_from_bits(vs2.iter().map(|value| *value < scalar as u8));
        run_mask_vx_u8::<VectorOpMsltu>(scalar, &vs2, &expected);

        let expected = mask_from_bits(vs2.iter().map(|value| (scalar as u8) > *value));
        run_mask_vx_u8::<VectorOpMsgtu>(scalar, &vs2, &expected);

        let signed_scalar = 0x7f;
        let expected = mask_from_bits(
            vs2.iter()
                .map(|value| as_signed_i128(signed_scalar as u8) > as_signed_i128(*value)),
        );
        run_mask_vx_u8::<VectorOpMsgt>(signed_scalar, &vs2, &expected);
    }

    #[test]
    fn test_vector_op_madc_msbc_mask_vvm() {
        let elem_count = VLEN_BYTE;
        let vs1: Vec<u8> = (0..elem_count)
            .map(|i| [1, 2, 0xff, 0x80, 0x7f, 0x10, 0xf0, 0x55][i % 8])
            .collect();
        let vs2: Vec<u8> = (0..elem_count)
            .map(|i| [0xff, 1, 1, 0x80, 0x80, 0xef, 0x20, 0x54][i % 8])
            .collect();
        let carry = mask_from_bits((0..elem_count).map(|i| i % 2 == 0));

        let expected = mask_from_bits(vs1.iter().zip(vs2.iter()).enumerate().map(
            |(index, (vs1, vs2))| {
                (*vs2 as u16 + *vs1 as u16 + mask_bit(&carry, index) as u16) > u8::MAX as u16
            },
        ));
        run_mask_vvm_u8::<VectorOpMadc>(&vs1, &vs2, &carry, &expected);

        let expected = mask_from_bits(vs1.iter().zip(vs2.iter()).enumerate().map(
            |(index, (vs1, vs2))| (*vs2 as u16) < (*vs1 as u16 + mask_bit(&carry, index) as u16),
        ));
        run_mask_vvm_u8::<VectorOpMsbc>(&vs1, &vs2, &carry, &expected);
    }

    #[test]
    fn test_vector_op_zext_vf2() {
        const LMUL: Vlmul = Vlmul::M1;
        const SRC_SEW: Vsew = Vsew::E8;
        const DST_SEW: Vsew = Vsew::E16;
        let param = TestOpParameter::new_v(8, 10, SRC_SEW, DST_SEW);

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
            param,
            |builder| {
                builder
                    .config(LMUL, DST_SEW, false, false, elem_count as u16)
                    .reg(LMUL.get_lmul(), param.vs2(), &vs2)
            },
            |checker| checker.reg(LMUL.get_lmul(), param.vd(), &expected),
        );
    }
}
