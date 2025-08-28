use std::hint::{cold_path, unreachable_unchecked};

use rustc_apfloat::{
    Float, FloatConvert, Status, StatusAnd,
    ieee::{Double, Single},
};

use rustc_apfloat::Round as APFloatRound;

use crate::{
    fpu::{Classification, Round},
    utils::{FloatPoint, TruncateFrom},
};

impl Into<APFloatRound> for Round {
    fn into(self) -> APFloatRound {
        match self {
            Round::NearestTiesToEven => APFloatRound::NearestTiesToEven,
            Round::TowardPositive => APFloatRound::TowardPositive,
            Round::TowardNegative => APFloatRound::TowardNegative,
            Round::TowardZero => APFloatRound::TowardZero,
            Round::NearestTiesToAway => APFloatRound::NearestTiesToAway,
        }
    }
}

#[derive(Clone, Copy)]
pub enum APFloat {
    Single(Single),
    Double(Double),
}

impl Into<APFloat> for f32 {
    fn into(self) -> APFloat {
        APFloat::Single(Single::from_bits(self.to_bits() as u128))
    }
}

impl Into<APFloat> for f64 {
    fn into(self) -> APFloat {
        APFloat::Double(Double::from_bits(self.to_bits() as u128))
    }
}

// Reinterpret to given type of soft float.
impl From<APFloat> for Single {
    fn from(value: APFloat) -> Self {
        match value {
            APFloat::Single(s) => s,
            APFloat::Double(d) => Single::from_bits(d.to_bits() as u128),
        }
    }
}

impl From<APFloat> for Double {
    fn from(value: APFloat) -> Self {
        match value {
            APFloat::Single(s) => Double::from_bits(s.to_bits() as u128),
            APFloat::Double(d) => d,
        }
    }
}

impl Into<APFloat> for Single {
    fn into(self) -> APFloat {
        APFloat::Single(self)
    }
}

impl Into<APFloat> for Double {
    fn into(self) -> APFloat {
        APFloat::Double(self)
    }
}

/// Map a Rust primitive float type to its `rustc_apfloat` representation.
pub trait APFloatOf: Into<APFloat> {
    type Float: Float + Into<APFloat> + From<APFloat>;
}

impl APFloatOf for f32 {
    type Float = Single;
}

impl APFloatOf for f64 {
    type Float = Double;
}

pub trait UnaryOp<F> {
    fn apply(a: F) -> F;
}

pub trait BinaryOp<F> {
    fn apply(a: F, b: F) -> F;
}

pub trait BinaryOpWithRound<F> {
    fn apply(a: F, b: F, round: Round) -> StatusAnd<F>;
}

pub trait TernaryOpWithRound<F> {
    fn apply(a: F, b: F, c: F, round: Round) -> StatusAnd<F>;
}

pub trait CmpOp<F> {
    fn apply(a: F, b: F) -> StatusAnd<bool>;
}

// Implementation of operations:

macro_rules! define_binary_op {
    ($struct_name:ident, $method_name:ident) => {
        pub struct $struct_name;
        impl<F: Float> BinaryOp<F> for $struct_name {
            fn apply(a: F, b: F) -> F {
                a.$method_name(b)
            }
        }
    };
}

macro_rules! define_binary_op_r {
    ($struct_name:ident, $method_name:ident) => {
        pub struct $struct_name;
        impl<F: Float> BinaryOpWithRound<F> for $struct_name {
            fn apply(a: F, b: F, round: Round) -> StatusAnd<F> {
                a.$method_name(b, round.into())
            }
        }
    };
}

// Arithmetic

define_binary_op_r!(AddOp, add_r);
define_binary_op_r!(SubOp, sub_r);
define_binary_op_r!(MulOp, mul_r);
define_binary_op_r!(DivOp, div_r);

pub struct MulAddOp;
impl<F: Float> TernaryOpWithRound<F> for MulAddOp {
    fn apply(a: F, b: F, c: F, round: Round) -> StatusAnd<F> {
        a.mul_add_r(b, c, round.into())
    }
}

pub struct MulSubOp;
impl<F: Float> TernaryOpWithRound<F> for MulSubOp {
    fn apply(a: F, b: F, c: F, round: Round) -> StatusAnd<F> {
        a.mul_add_r(b, -c, round.into())
    }
}

pub struct NegMulAddOp;
impl<F: Float> TernaryOpWithRound<F> for NegMulAddOp {
    fn apply(a: F, b: F, c: F, round: Round) -> StatusAnd<F> {
        (-a).mul_add_r(b, c, round.into())
    }
}

pub struct NegMulSubOp;
impl<F: Float> TernaryOpWithRound<F> for NegMulSubOp {
    fn apply(a: F, b: F, c: F, round: Round) -> StatusAnd<F> {
        (-a).mul_add_r(b, -c, round.into())
    }
}

pub struct SqrtOp;
impl<F: Float> UnaryOp<F> for SqrtOp {
    fn apply(a: F) -> F {
        // TODO: rustc_apfloat doesn't provide sqrt;
        // current implementation may not comply with IEEE 754.
        F::from_bits(f64::from_bits(a.to_bits() as u64).sqrt().to_bits() as u128)
    }
}

define_binary_op!(MinOp, min);
define_binary_op!(MaxOp, max);

// Sign injection

define_binary_op!(SignInjectOp, copy_sign);

pub struct SignInjectNegOp;
impl<F: Float> BinaryOp<F> for SignInjectNegOp {
    fn apply(a: F, b: F) -> F {
        if a.is_negative() == b.is_negative() {
            -a
        } else {
            a
        }
    }
}

pub struct SignInjectXorOp;
impl<F: Float> BinaryOp<F> for SignInjectXorOp {
    fn apply(a: F, b: F) -> F {
        if a.is_negative() != b.is_negative() {
            a.abs().neg()
        } else {
            a.abs()
        }
    }
}

// Classify (RISC-V)
fn classify<F: Float>(f: F) -> Classification {
    if f.is_normal() {
        if f.is_negative() {
            Classification::NormalNegative
        } else {
            Classification::NormalPositive
        }
    } else {
        cold_path();
        if f.is_denormal() {
            if f.is_negative() {
                Classification::SubnormalNegative
            } else {
                Classification::SubnormalPositive
            }
        } else if f.is_zero() {
            if f.is_negative() {
                Classification::NegativeZero
            } else {
                Classification::PositiveZero
            }
        } else if f.is_infinite() {
            if f.is_negative() {
                Classification::NegativeInfinity
            } else {
                Classification::PositiveInfinity
            }
        } else if f.is_nan() {
            if f.is_signaling() {
                Classification::SignalingNaN
            } else {
                Classification::QuietNaN
            }
        } else {
            unsafe {
                unreachable_unchecked();
            }
        }
    }
}

// Compare
pub struct EqOp;
impl<F: Float> CmpOp<F> for EqOp {
    fn apply(a: F, b: F) -> StatusAnd<bool> {
        // `feq` do "quiet comparison", according to RISC-V manual 2025-08-05,
        // which means only sets the invalid op if either input is a signaling NaN.
        if a.is_signaling() || b.is_signaling() {
            Status::INVALID_OP.and(false)
        } else {
            Status::OK.and(a == b)
        }
    }
}

pub struct LtOp;
impl<F: Float> CmpOp<F> for LtOp {
    fn apply(a: F, b: F) -> StatusAnd<bool> {
        // Unlike `feq`, `flt` do "signaling comparison",
        // that means set the invalid operation exception flag if either input is NaN.
        if a.is_nan() || b.is_nan() {
            Status::INVALID_OP.and(false)
        } else {
            Status::OK.and(a < b)
        }
    }
}

pub struct LeOp;
impl<F: Float> CmpOp<F> for LeOp {
    fn apply(a: F, b: F) -> StatusAnd<bool> {
        // Same with `flt`
        if a.is_nan() || b.is_nan() {
            Status::INVALID_OP.and(false)
        } else {
            Status::OK.and(a <= b)
        }
    }
}

pub struct SoftFPU {
    last_status: std::cell::Cell<Status>,
    reg_file: [APFloat; 32],
}

impl SoftFPU {
    pub fn new() -> Self {
        Self {
            last_status: std::cell::Cell::new(Status::OK),
            reg_file: [APFloat::Single(Single::from_bits(0)); 32],
        }
    }

    pub fn last_status(&self) -> Status {
        self.last_status.get()
    }

    fn save_and_unwrap<T>(&mut self, status_and: StatusAnd<T>) -> T {
        self.last_status.set(status_and.status);
        status_and.value
    }

    // Reinterpret current register value to `Single`.
    fn get_as_single(&self, index: u8) -> Single {
        self.reg_file[index as usize].into()
    }

    fn get_as_double(&self, index: u8) -> Double {
        self.reg_file[index as usize].into()
    }

    pub fn get_f32(&self, index: u8) -> f32 {
        f32::from_bits(self.get_as_single(index).to_bits() as u32)
    }

    pub fn get_f64(&self, index: u8) -> f64 {
        f64::from_bits(self.get_as_double(index).to_bits() as u64)
    }

    pub fn set_f32(&mut self, index: u8, value: f32) {
        self.reg_file[index as usize] = APFloat::Single(Single::from_bits(value.to_bits() as u128));
    }

    pub fn set_f64(&mut self, index: u8, value: f64) {
        self.reg_file[index as usize] = APFloat::Double(Double::from_bits(value.to_bits() as u128));
    }

    pub fn load<F: FloatPoint>(&self, index: u8) -> F {
        let f: <F as APFloatOf>::Float = self.reg_file[index as usize].into();
        F::from_bits(F::BitsType::truncate_from(f.to_bits()))
    }

    pub fn store<F: Into<APFloat>>(&mut self, index: u8, value: F) {
        self.reg_file[index as usize] = value.into();
    }

    pub fn store_from_bits<F: APFloatOf>(&mut self, index: u8, value: u128) {
        self.reg_file[index as usize] = F::Float::from_bits(value).into();
    }

    pub fn cvt_u_to_f_and_store<F: FloatPoint>(&mut self, index: u8, value: u128, round: Round) {
        let rst = <F as APFloatOf>::Float::from_u128_r(value, round.into());
        let val = self.save_and_unwrap(rst);
        self.reg_file[index as usize] = val.into();
    }

    pub fn cvt_s_to_f_and_store<F: FloatPoint>(&mut self, index: u8, value: i128, round: Round) {
        let rst = <F as APFloatOf>::Float::from_i128_r(value, round.into());
        let val = self.save_and_unwrap(rst);
        self.reg_file[index as usize] = val.into();
    }

    pub fn get_and_cvt_unsigned<F: APFloatOf>(&mut self, index: u8, round: Round) -> u128 {
        let mut in_exact = true;
        let f: F::Float = self.reg_file[index as usize].into();

        let StatusAnd::<_> { mut status, value } = f.to_u128_r(32, round.into(), &mut in_exact);

        if in_exact {
            status |= Status::INEXACT;
        }
        self.last_status.set(status);
        value
    }

    pub fn get_and_cvt_signed<F: APFloatOf>(&mut self, index: u8, round: Round) -> i128 {
        let mut in_exact = true;
        let f: F::Float = self.reg_file[index as usize].into();

        let StatusAnd::<_> { mut status, value } = f.to_i128_r(32, round.into(), &mut in_exact);

        if in_exact {
            status |= Status::INEXACT;
        }
        self.last_status.set(status);
        value
    }
    pub fn get_and_cvt_i64<F: APFloatOf>(&mut self, index: u8, round: Round) -> i64 {
        self.get_and_cvt_signed::<F>(index, round) as i64
    }

    pub fn get_and_cvt_i32<F: APFloatOf>(&mut self, index: u8, round: Round) -> i32 {
        self.get_and_cvt_signed::<F>(index, round) as i32
    }

    pub fn float_convert_r<F: APFloatOf, T: APFloatOf>(
        &mut self,
        index: u8,
        round: Round,
    ) -> T::Float
    where
        F::Float: FloatConvert<T::Float>,
    {
        let mut _loses_info = true;
        let f: F::Float = self.reg_file[index as usize].into();
        self.save_and_unwrap(f.convert_r(round.into(), &mut _loses_info))
    }

    pub fn classify<T: APFloatOf>(&self, rs: u8) -> Classification {
        let f: T::Float = self.reg_file[rs as usize].into();
        classify(f)
    }

    pub fn compare<Op: CmpOp<T::Float>, T: FloatPoint>(&mut self, rs1: u8, rs2: u8) -> bool {
        let a: T::Float = self.reg_file[rs1 as usize].into();
        let b: T::Float = self.reg_file[rs2 as usize].into();
        self.save_and_unwrap(Op::apply(a, b))
    }

    pub fn exec_unary<Op, T: FloatPoint>(&mut self, rs1: u8, rd: u8)
    where
        Op: UnaryOp<T::Float>,
    {
        let a: T::Float = self.reg_file[rs1 as usize].into();
        self.reg_file[rd as usize] = Op::apply(a).into();
    }

    pub fn exec_binary<Op, T: FloatPoint>(&mut self, rs1: u8, rs2: u8, rd: u8)
    where
        Op: BinaryOp<T::Float>,
    {
        let a: T::Float = self.reg_file[rs1 as usize].into();
        let b: T::Float = self.reg_file[rs2 as usize].into();
        self.reg_file[rd as usize] = Op::apply(a, b).into();
    }

    pub fn exec_binary_r<Op, T: FloatPoint>(&mut self, rs1: u8, rs2: u8, rd: u8, round: Round)
    where
        Op: BinaryOpWithRound<<T as APFloatOf>::Float>,
    {
        let a: T::Float = self.reg_file[rs1 as usize].into();
        let b: T::Float = self.reg_file[rs2 as usize].into();
        self.reg_file[rd as usize] = self.save_and_unwrap(Op::apply(a, b, round)).into();
    }

    pub fn exec_ternary_r<Op, T: FloatPoint>(
        &mut self,
        rs1: u8,
        rs2: u8,
        rs3: u8,
        rd: u8,
        round: Round,
    ) where
        Op: TernaryOpWithRound<T::Float>,
    {
        let a: T::Float = self.reg_file[rs1 as usize].into();
        let b: T::Float = self.reg_file[rs2 as usize].into();
        let c: T::Float = self.reg_file[rs3 as usize].into();
        self.reg_file[rd as usize] = self.save_and_unwrap(Op::apply(a, b, c, round)).into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_arith() {
        let mut fpu = SoftFPU::new();
        fpu.set_f32(1, 2.0f32);
        fpu.set_f32(2, 3.0f32);

        // add
        fpu.exec_binary_r::<AddOp, f32>(1, 2, 3, Round::NearestTiesToEven);
        assert_eq!(fpu.get_f32(3), 5.0f32);

        // sub
        fpu.exec_binary_r::<SubOp, f32>(2, 1, 6, Round::NearestTiesToEven);
        assert_eq!(fpu.get_f32(6), 1.0f32);

        // mul
        fpu.exec_binary_r::<MulOp, f32>(1, 2, 4, Round::NearestTiesToEven);
        assert_eq!(fpu.get_f32(4), 6.0f32);

        // div
        fpu.exec_binary_r::<DivOp, f32>(2, 1, 5, Round::NearestTiesToEven);
        assert_eq!(fpu.get_f32(5), 1.5f32);
    }

    #[test]
    fn test_ternary_mul_add() {
        let mut fpu = SoftFPU::new();
        fpu.set_f64(1, 1.5f64);
        fpu.set_f64(2, 2.0f64);
        fpu.set_f64(3, 0.5f64);

        fpu.exec_ternary_r::<MulAddOp, f64>(1, 2, 3, 4, Round::NearestTiesToEven);
        assert_eq!(fpu.get_f64(4), 1.5f64 * 2.0f64 + 0.5f64);
    }

    #[test]
    fn test_classify() {
        let mut fpu = SoftFPU::new();

        fpu.set_f32(1, 0.0f32);
        assert!(matches!(
            fpu.classify::<f32>(1),
            Classification::PositiveZero
        ));

        fpu.set_f32(1, -0.0f32);
        assert!(matches!(
            fpu.classify::<f32>(1),
            Classification::NegativeZero
        ));

        fpu.set_f32(2, f32::INFINITY);
        assert!(matches!(
            fpu.classify::<f32>(2),
            Classification::PositiveInfinity
        ));

        fpu.set_f32(2, f32::NEG_INFINITY);
        assert!(matches!(
            fpu.classify::<f32>(2),
            Classification::NegativeInfinity
        ));

        fpu.set_f32(3, f32::NAN);
        let c = fpu.classify::<f32>(3);
        assert!(matches!(
            c,
            Classification::QuietNaN | Classification::SignalingNaN
        ));
    }

    #[test]
    fn test_subnormal_and_sign() {
        let mut fpu = SoftFPU::new();

        // smallest positive subnormal for f32
        fpu.set_f32(1, f32::from_bits(0x0000_0001));
        assert!(matches!(
            fpu.classify::<f32>(1),
            Classification::SubnormalPositive
        ));

        // smallest negative subnormal
        fpu.set_f32(1, f32::from_bits(0x8000_0001));
        assert!(matches!(
            fpu.classify::<f32>(1),
            Classification::SubnormalNegative
        ));
    }

    #[test]
    fn test_convert_r_i32() {
        use rustc_apfloat::Status;

        let mut fpu = SoftFPU::new();

        fpu.set_f32(1, 1.9f32);

        let n = fpu.get_and_cvt_i32::<f32>(1, Round::NearestTiesToEven);
        assert_eq!(n, 2);

        let z = fpu.get_and_cvt_i32::<f32>(1, Round::TowardZero);
        assert_eq!(z, 1);

        fpu.set_f32(2, -1.9f32);
        let nz = fpu.get_and_cvt_i32::<f32>(2, Round::TowardZero);
        assert_eq!(nz, -1);
        let nd = fpu.get_and_cvt_i32::<f32>(2, Round::TowardNegative);
        assert_eq!(nd, -2);

        fpu.set_f32(3, 1.0f32 / 3.0f32);
        let _ = fpu.get_and_cvt_i32::<f32>(3, Round::NearestTiesToEven);
        assert!(fpu.last_status.get().contains(Status::INEXACT));
    }

    #[test]
    fn test_div_inexact_status() {
        use rustc_apfloat::Status;

        let mut fpu = SoftFPU::new();
        fpu.set_f32(1, 1.0f32);
        fpu.set_f32(2, 3.0f32);

        // 1/3 is inexact in binary32
        fpu.exec_binary_r::<DivOp, f32>(1, 2, 3, Round::NearestTiesToEven);
        let r = fpu.get_f32(3);

        assert!((r - (1.0f32 / 3.0f32)).abs() < 1e-7);
        assert!(fpu.last_status.get().contains(Status::INEXACT));
    }

    #[test]
    fn test_float_convert() {
        // f32 -> f64 conversion should preserve value for a simple value
        let mut fpu = SoftFPU::new();
        fpu.set_f32(1, 1.5f32);

        let d = fpu.float_convert_r::<f32, f64>(1, Round::NearestTiesToEven);

        let as_u64 = d.to_bits() as u64;
        assert_eq!(as_u64, f64::from(1.5f32).to_bits());
    }

    #[test]
    fn test_float_cmp() {
        let mut fpu = SoftFPU::new();

        fpu.store::<f32>(1, 3.0);
        fpu.store::<f32>(2, 3.0);
        assert!(fpu.compare::<EqOp, f32>(1, 2));
        assert!(fpu.last_status() == Status::OK);

        fpu.store::<f32>(1, f32::NAN);
        fpu.store::<f32>(2, 3.0);
        assert!(fpu.compare::<EqOp, f32>(1, 2) == false);
        assert!(fpu.last_status() == Status::OK);

        fpu.store_from_bits::<f32>(1, Single::snan(None).to_bits());
        fpu.store::<f32>(2, 3.0);
        assert!(fpu.compare::<EqOp, f32>(1, 2) == false);
        assert!(fpu.last_status() == Status::INVALID_OP);

        fpu.store::<f32>(1, 1.5);
        fpu.store::<f32>(2, 3.0);
        assert!(fpu.compare::<LtOp, f32>(1, 2));
        assert!(fpu.last_status() == Status::OK);

        fpu.store::<f32>(1, f32::NAN);
        fpu.store::<f32>(2, 3.0);
        assert!(fpu.compare::<LtOp, f32>(1, 2) == false);
        assert!(fpu.last_status() == Status::INVALID_OP);

        fpu.store_from_bits::<f32>(1, Single::snan(None).to_bits());
        fpu.store::<f32>(2, 3.0);
        assert!(fpu.compare::<LtOp, f32>(1, 2) == false);
        assert!(fpu.last_status() == Status::INVALID_OP);
    }
}
