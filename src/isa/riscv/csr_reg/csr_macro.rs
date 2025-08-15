use super::CsrReg;
use crate::config::arch_config::{SignedWordType, WordType};

/// Generator a single csr register.
macro_rules! gen_csr_reg {
    (
        $name:ident, $addr:expr, $default:expr,
        [ $( $bit:expr, $len:expr, $fname:ident ),*  $(,)? ]
    ) => {
        pub struct $name {
            data: *mut WordType,
        }

        impl From<*mut WordType> for $name {
            fn from(value: *mut WordType) -> Self {
                Self { data: value }
            }
        }

        impl CsrReg for $name {
            fn get_index() -> WordType {
                $addr
            }
        }

        impl $name {
            $(
                pub fn ${concat(get_, $fname)}(&self) -> WordType {
                    const LOW_BIT: WordType = if ($bit >= 0) {
                        ($bit as SignedWordType).abs() as WordType
                    }
                    else {
                        ((size_of::<WordType>() as SignedWordType) + $bit) as WordType
                    };

                    ((unsafe { self.data.read_volatile() })
                    & ((WordType::from(1u8) << $len) - 1) << LOW_BIT) >> LOW_BIT
                }

                pub fn ${concat(set_, $fname)}(&self, val: WordType) {
                    assert!(val < (1 << $len));
                    const LOW_BIT: WordType = if ($bit >= 0) {
                        ($bit as SignedWordType).abs() as WordType
                    }
                    else {
                        ((size_of::<WordType>() as SignedWordType) + $bit) as WordType
                    };

                    let mut data = unsafe { self.data.read_volatile() };
                    data &= !((((WordType::from(1u8) << $len) - 1) as WordType) << LOW_BIT);
                    unsafe { self.data.write_volatile(data | (val << LOW_BIT)) };
                }
            )*
        }
    };
}

/// Generator csr RegFile.
macro_rules! gen_csr_regfile {
    (
        $( $name:ident, $addr:expr, $default:expr, [ $( $bit:expr, $len:expr, $fname:ident ),* $(,)? ] );* $(;)?
    ) => {
        $(
            gen_csr_reg!($name, $addr, $default, [ $( $bit, $len, $fname ),* ]);
        )*

        pub const CSR_REG_TABLE: &[(WordType, WordType)] = &[
            $(
                ($addr, $default)
            ),*
        ];
    };
}

gen_csr_regfile! {
    Mstatus, 0x300, 0x00, [
        1,  1, sie,
        3,  1, mie,
        5,  1, spie,
        6,  1, ube,
        7,  1, mpie,
        8,  1, spp,
        9,  2, vs,
        11, 2, mpp,
        13, 2, fs,
        15, 2, xs,
        17, 1, mprv,
        18, 1, sum,
        19, 1, mxr,
        20, 1, tvm,
        21, 1, tw,
        22, 1, tsr,
        23, 1, spelp,
        24, 1, sdt,
        32, 2, xul,
        34, 2, sxl,
        36, 1, sbe,
        37, 1, mbe,
        38, 1, gva,
        39, 1, mpv,
        40, 1, wpri,
        41, 1, mpelp,
        42, 1, mdt,
        -1, 1, sd,
    ];

    Misa, 0x301, 0x00, [
        0, 25, extension,
        -2, 2, mxl,
    ];

    Mie, 0x304, 0x00, [
        0,  1, usie,
        1,  1, ssie,
        2,  1, msie,
        4,  1, utie,
        5,  1, stie,
        6,  1, mtie,
        8,  1, ueie,
        9,  1, seie,
        10, 1, meie,
        0, size_of::<WordType>(), mip
    ];

    Mtvec, 0x305, 0x00, [
        0, 2, mode,
        2, size_of::<WordType>() - 2, base,
    ];

    Mcratch, 0x340, 0x00, [
        0, size_of::<WordType>(), mscratch,
    ];

    Mepc, 0x341, 0x00, [
        0, size_of::<WordType>(), mepc,
    ];

    Mcause, 0x342, 0x00, [
        0, size_of::<WordType>() - 1, exception_code,
        -1, 1, interrupt,
    ];

    Mtval, 0x343, 0x00, [
        0, size_of::<WordType>(), mtval,
    ];

    Mip, 0x344, 0x00, [
        0,  1, usip,
        1,  1, ssip,
        2,  1, msip,
        4,  1, utip,
        5,  1, stip,
        6,  1, mtip,
        8,  1, ueip,
        9,  1, seip,
        10, 1, meip,
        0, size_of::<WordType>(), mip,
    ];
}
