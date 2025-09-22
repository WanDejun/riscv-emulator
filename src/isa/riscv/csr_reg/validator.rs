use super::*;
use crate::{config::arch_config::WordType, utils::BIT_ONES_ARRAY};

pub(super) type Validator = fn(WordType, &CsrContext) -> CsrWriteOp;

#[inline]
const fn extract_bits(value: WordType, left: usize, right: usize) -> WordType {
    let width = right - left + 1;
    (value >> left) & BIT_ONES_ARRAY[width]
}

#[inline]
const fn make_mask(left: usize, right: usize) -> WordType {
    let width = right - left + 1;
    BIT_ONES_ARRAY[width] << left
}

/// Condition trait for [`make_validator_with_cond`] helper functions.
trait ValidateCond {
    fn check(value: WordType, ctx: &CsrContext) -> bool;
}

struct NeverCond {}
impl ValidateCond for NeverCond {
    fn check(_value: WordType, _ctx: &CsrContext) -> bool {
        false
    }
}

struct RangeCond<const MIN: WordType, const MAX: WordType> {}
impl<const MIN: WordType, const MAX: WordType> ValidateCond for RangeCond<MIN, MAX> {
    fn check(value: WordType, _ctx: &CsrContext) -> bool {
        return MIN <= value && value <= MAX;
    }
}

macro_rules! make_enum_cond {
    ($name:ident, $($valid:expr),+ $(,)?) => {
    struct $name {}
    impl ValidateCond for $name {
            fn check(value: WordType, _ctx: &CsrContext) -> bool {
                const VALID_VALUES: &[WordType] = &[$($valid),+];
                VALID_VALUES.contains(&value)
            }
        }
    }
}

/// Make a validator that only write when the condition is satisfied, and do nothing otherwise.
fn make_validator_with_cond<const L: usize, const R: usize, C: ValidateCond>() -> Validator {
    |value, ctx| -> CsrWriteOp {
        let mask = make_mask(L, R);
        let extracted = extract_bits(value, L, R);
        if C::check(extracted, ctx) {
            CsrWriteOp {
                mask,
                value: extracted,
            }
        } else {
            CsrWriteOp { mask: 0, value: 0 }
        }
    }
}

fn make_range_validator<
    const L: usize,
    const R: usize,
    const MIN: WordType,
    const MAX: WordType,
>() -> Validator {
    make_validator_with_cond::<L, R, RangeCond<MIN, MAX>>()
}

fn make_readonly_validator<const L: usize, const R: usize>() -> Validator {
    make_validator_with_cond::<L, R, NeverCond>()
}

/// Make a validator that only write the bits with masks.
fn make_mask_validator<const L: usize, const R: usize, const MASK: WordType>() -> Validator {
    |value, _ctx| -> CsrWriteOp {
        let mask = make_mask(L, R);
        let extracted = extract_bits(value, L, R);
        CsrWriteOp {
            mask,
            value: extracted & MASK,
        }
    }
}

macro_rules! combine_validators {
    ($value:expr, $ctx:expr, $($validator:expr),+ $(,)?) => {
        {
            let mut combined = CsrWriteOp { mask: 0, value: 0 };
            $(
                let result = $validator($value, $ctx);
                combined = combined.merge(&result);
            )+
            combined
        }
    };
}

pub(super) fn validate_never_write(_value: WordType, _ctx: &CsrContext) -> CsrWriteOp {
    CsrWriteOp { mask: 0, value: 0 }
}

fn validate_misa_extension(value: WordType, ctx: &CsrContext) -> CsrWriteOp {
    // TODO: We need to check extension combination for example, if 'D' is set, 'F' must be set too.
    CsrWriteOp::new(make_mask(0, 25), value & ctx.extension)
}

pub(super) fn validate_misa(value: WordType, ctx: &CsrContext) -> CsrWriteOp {
    // TODO: We don't actually support changing extensions right now.
    make_readonly_validator::<0, 63>()(value, ctx)
    // combine_validators!(
    //     value,
    //     ctx,
    //     make_range_validator::<62, 63, 1, 3>(), // TODO: This only works in 64-bit mode
    //     make_readonly_validator::<26, 61>(),
    //     validate_misa_extension,
    // )
}
