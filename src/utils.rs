use std::usize;

use crate::config::arch_config::WordType;

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

const ALIGN_ILST: [WordType; 9] = [
    0x0,
    0x1,
    0x03,
    WordType::MAX,
    0x07,
    WordType::MAX,
    WordType::MAX,
    WordType::MAX,
    0x15,
];

pub unsafe fn read_raw_ptr<T>(addr: *const u8) -> T {
    let size_of_t: usize = size_of::<T>();
    assert!(
        (addr as WordType) & ALIGN_ILST[size_of_t] == 0,
        "read_word -> addr: {}, is not aligned!",
        addr as usize
    );

    let ptr = addr as *const T;

    unsafe { ptr.read() }
}

pub unsafe fn write_raw_ptr<T>(addr: *mut u8, data: T) {
    let size_of_t: usize = size_of::<T>();
    assert!(
        (addr as WordType) & ALIGN_ILST[size_of_t] == 0,
        "read_word -> addr: {}, is not aligned!",
        addr as usize
    );

    let ptr = addr as *mut T;
    unsafe { ptr.write(data) }
}

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

#[macro_export]
/**  NOTE: make sure you have use [`crate::utils::gen_name_list`] in the same namespace. */
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
