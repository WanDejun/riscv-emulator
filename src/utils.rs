use std::{
    fmt::{Debug, Display},
    ops::*,
    usize,
};

use crate::config::arch_config::WordType;

fn rand_unique<T, F>(rd: F, cnt: usize) -> Vec<T>
where
    T: Copy + Eq + std::hash::Hash + std::fmt::Debug,
    F: Fn() -> T,
{
    let mut set = std::collections::HashSet::new();
    let mut result = Vec::with_capacity(cnt);

    while result.len() < cnt {
        let val = rd();
        if set.insert(val) {
            result.push(val);
        }
    }

    result
}

#[macro_export]
macro_rules! concat_bits {
    // End case (only one argument)
    ($t:ty; $last:expr) => {
        ($last as u64)
    };

    // Recursive concatenation (left shift * number of bits)
    ($t:ty; $head:expr, $($tail:expr),+) => {
        (($head as u64) << (8 * $crate::count_args!($($tail),+))) |
        concat_bits!($t; $($tail),+)
    };
}

#[macro_export]
macro_rules! count_args {
    ($($x:expr),*) => {
        <[()]>::len(&[$(count_args![@sub $x]),*])
    };
    (@sub $_:expr) => { () };
}

// ==================================
// read raw ptr ans check align.
// ==================================
const ALIGN_ILST: [WordType; 9] = [
    0x00,
    0x00,
    0x01,
    WordType::MAX,
    0x03,
    WordType::MAX,
    WordType::MAX,
    WordType::MAX,
    0x07,
];

pub unsafe fn read_raw_ptr<T>(addr: *const u8) -> T {
    let size_of_t: usize = size_of::<T>();
    assert!(
        (addr as WordType) & ALIGN_ILST[size_of_t] == 0,
        "read_word -> addr: {}, is not aligned!",
        addr as usize
    );

    let ptr = addr as *const T;

    unsafe { ptr.read_volatile() }
}

pub unsafe fn write_raw_ptr<T>(addr: *mut u8, data: T) {
    let size_of_t: usize = size_of::<T>();
    assert!(
        (addr as WordType) & ALIGN_ILST[size_of_t] == 0,
        "read_word -> addr: {}, is not aligned!",
        addr as usize
    );

    let ptr = addr as *mut T;
    unsafe { ptr.write_volatile(data) }
}

pub fn check_align<T>(addr: WordType) -> bool {
    let size_of_t: usize = size_of::<T>();
    (addr as WordType) & ALIGN_ILST[size_of_t] == 0
}

// ========================================
//  gen_name_list ["a1", "a2", "a3", ... ]
// ========================================

/// # Examples
/// ```
/// assert_eq!(gen_name_list("a"; 0, 5), ["a0", "a1", "a2", "a3", "a4", "a5"])
/// ```
#[macro_export]
macro_rules! gen_name_list {
    ($base:literal; $begin: literal, $end: literal) => {
        seq_macro::seq!(N in $begin..= $end {
            [ #(concat!($base, stringify!(N)),) *]
        })
    }
}

pub const fn concat_arrays<const SIZE1: usize, const SIZE2: usize>(
    arr1: [&'static str; SIZE1],
    arr2: [&'static str; SIZE2],
) -> [&'static str; SIZE1 + SIZE2] {
    let mut i = 0;
    let mut j = 0;
    let mut k = 0;
    let mut ret: [&'static str; SIZE1 + SIZE2] = [""; SIZE1 + SIZE2];
    while i < SIZE1 {
        ret[k] = arr1[i];
        i += 1;
        k += 1;
    }
    while j < SIZE2 {
        ret[k] = arr2[j];
        j += 1;
        k += 1;
    }
    ret
}

/// # NOTE
/// make sure you have use [`crate::utils::gen_name_list`] in the same namespace.
/// # Examples
/// ```
/// assert_eq!(gen_reg_name_list("s"; 0, 2; "t"; 1, 3), ["s0", "s1", "s2", "t1", "t2", "t3"])
/// ```
#[macro_export]
macro_rules! gen_reg_name_list {
    ($base:literal, $begin: literal, $end: literal; $($rest:tt)*) => {
        crate::utils::concat_arrays(gen_name_list!($base; $begin, $end), gen_reg_name_list!($($rest)*))
    };

    ($base:literal, $begin: literal, $end: literal) => {
        gen_name_list!($base; $begin, $end)
    };

    ($name:literal; $($rest:tt)*) => {
        crate::utils::concat_arrays([$name], gen_reg_name_list!($($rest)*))
    };

    ($name:literal) => {
        [$name]
    };
}

pub trait TruncateFrom<T>: Sized {
    fn truncate_from(value: T) -> Self;
}

pub trait TruncateTo<T>: Sized {
    fn truncate_to(self, bits: u32) -> Self;
}

macro_rules! impl_truncate_from {
    ($from:ty, $to:ty) => {
        impl TruncateFrom<$from> for $to {
            fn truncate_from(val: $from) -> Self {
                val as $to
            }
        }
    };
}

macro_rules! impl_truncate_to {
    ($T:ty) => {
        impl TruncateTo<$T> for $T {
            fn truncate_to(self, bits: u32) -> Self {
                self & ((1 << bits) - 1)
            }
        }
    };
}

impl_truncate_from!(WordType, u8);
impl_truncate_from!(WordType, u16);
impl_truncate_from!(WordType, u32);
impl_truncate_from!(WordType, u64);

impl_truncate_to!(u8);
impl_truncate_to!(u16);
impl_truncate_to!(u32);
impl_truncate_to!(u64);

pub trait UnsignedInteger:
    Copy
    + Sized
    + From<u8>
    + Into<u64>
    // 算术运算符
    + Add<Output = Self>
    + AddAssign
    + Sub<Output = Self>
    + SubAssign
    + Mul<Output = Self>
    + MulAssign
    + Div<Output = Self>
    + DivAssign
    // 位运算符
    + BitAnd<Output = Self>
    + BitAndAssign
    + BitOr<Output = Self>
    + BitOrAssign
    + BitXor<Output = Self>
    + BitXorAssign
    + Not<Output = Self>
    // 位移操作（右/左移可能和 u32、usize 混用）
    + Shl<u32, Output = Self>
    + ShlAssign<u32>
    + Shr<u32, Output = Self>
    + ShrAssign<u32>
    + PartialEq
    + Eq
    + PartialOrd
    + Ord
    + Debug
    + Display
    + TruncateFrom<WordType>
{
}
impl UnsignedInteger for u8 {}
impl UnsignedInteger for u16 {}
impl UnsignedInteger for u32 {}
impl UnsignedInteger for u64 {}

#[allow(unused)]
pub fn set_bit<T>(data: &mut T, idx: u32)
where
    T: BitOrAssign + From<u8> + Shl<u32, Output = T> + Copy,
{
    *data |= T::from(1u8) << idx;
}

#[allow(unused)]
pub fn clear_bit<T>(data: &mut T, idx: u32)
where
    T: BitAndAssign + From<u8> + Shl<u32, Output = T> + Copy + Not<Output = T>,
{
    *data &= !(T::from(1u8) << idx);
}

#[allow(unused)]
pub fn read_bit<T>(data: &T, idx: u32) -> bool
where
    T: BitAnd<Output = T> + From<u8> + Shl<u32, Output = T> + Copy + Not<Output = T> + Eq,
{
    (*data & (T::from(1u8) << idx)) != T::from(0u8)
}
