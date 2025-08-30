use phf::phf_map;

use super::CsrReg;
use crate::config::arch_config::{SignedWordType, WordType, XLEN};
use crate::utils::BIT_ONES_ARRAY;

/// Generator a single csr register.
macro_rules! gen_csr_reg {
    (
        $name:ident, $addr:expr,
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
            fn data(&self) -> WordType {
                unsafe { *(self.data) }
            }

            fn get_index() -> WordType {
                $addr
            }
            fn clear_by_mask(&mut self, mask: WordType) {
                unsafe {*self.data &= !(mask)}
            }
            fn set_by_mask(&mut self, mask: WordType) {
                unsafe {*self.data |= mask}
            }
        }

        impl $name {
            $(
                #[inline]
                pub fn ${concat(get_, $fname)}(&self) -> WordType {
                    const LOW_BIT: WordType = if ($bit >= 0) {
                        ($bit as SignedWordType).abs() as WordType
                    }
                    else {
                        ((XLEN as SignedWordType) + $bit) as WordType
                    };

                    ((unsafe { self.data.read_volatile() })
                    & (BIT_ONES_ARRAY[$len]) << LOW_BIT) >> LOW_BIT
                }

                #[inline]
                pub fn ${concat(set_, $fname)}(&self, val: WordType) {
                    assert!(val <= BIT_ONES_ARRAY[$len]);
                    const LOW_BIT: WordType = if ($bit >= 0) {
                        ($bit as SignedWordType).abs() as WordType
                    }
                    else {
                        ((XLEN as SignedWordType) + $bit) as WordType
                    };

                    let mut data = unsafe { self.data.read_volatile() };
                    data &= !((BIT_ONES_ARRAY[$len]) << LOW_BIT);
                    unsafe { self.data.write_volatile(data | (val << LOW_BIT)) };
                }
            )*
        }
    };
}

macro_rules! gen_csr_name_hashmap {
    ($(($name: literal, $addr: expr)),* $(,)? ) => {
        pub const CSR_NAME: phf::Map<&'static str, WordType> = phf_map! {
            $(
                $name => $addr
            ),*
        };
    };
}

/// Generator csr RegFile.
macro_rules! gen_csr_regfile {
    (
        $( $name:ident, $name_str: literal, $addr:expr, $default:expr, [ $( $bit:expr, $len:expr, $fname:ident ),* $(,)? ] );* $(;)?
    ) => {
        $(
            gen_csr_reg!($name, $addr, [ $( $bit, $len, $fname ),* ]);
        )*

        pub const CSR_REG_TABLE: &[(WordType, WordType)] = &[
            $(
                ($addr, $default)
            ),*
        ];

        gen_csr_name_hashmap!($(($name_str, $addr)),*);
    };
}

// gen_csr_name_hashmap!(("mstatus", 0x300),);

gen_csr_regfile! {
    // ==================================
    //            U-Mode CSR
    // ==================================
    Fcsr, "fcsr", 0x003, 0x00, [
        0, 5, fflags,
        0, 1, nx,
        1, 1, uf,
        2, 1, of,
        3, 1, dz,
        4, 1, nv,

        // rounding  mode
        5, 3, rm,
    ];

    // ==================================
    //            S-Mode CSR
    // ==================================
    Sstatus, "sstatus", 0x100, 0x00, [
        1,  1, sie,
        5,  1, spie,
        6,  1, ube,
        8,  1, spp,
        9,  2, vs,
        13, 2, fs,
        15, 2, xs,
        18, 1, sum,
        19, 1, mxr,
        23, 1, spelp,
        24, 1, sdt,
        32, 2, xul,
        -1, 1, sd,
    ];

    Sie, "sie", 0x104, 0x00, [
        0,  1, usie, // User Software Interrupt Enable
        1,  1, ssie,
        4,  1, utie, // User Time     Interrupt Enable
        5,  1, stie,
        8,  1, ueie, // User External Interrupt Enable
        9,  1, seie,
        0, XLEN, mip
    ];

    Stvec, "stvec", 0x105, 0x00, [
        0, 2, mode,
        2, XLEN - 2, base,
    ];

    Sscratch, "sscratch", 0x140, 0x00, [
        0, XLEN, mscratch,
    ];

    Sepc, "sepc", 0x141, 0x00, [
        0, XLEN, sepc,
    ];

    Scause, "scause", 0x142, 0x00, [
        0, XLEN - 1, exception_code,
        -1, 1, interrupt,
    ];

    Stval, "stval", 0x143, 0x00, [
        0, XLEN, stval,
    ];

    Sip, "sip", 0x144, 0x00, [
        0,  1, usip, // User Software Interrupt Pending.
        1,  1, ssip,
        // 2,  1, hsip,
        4,  1, utip, // User Time     Interrupt Pending.
        5,  1, stip,
        // 6,  1, htip,
        8,  1, ueip, // User External Interrupt Pending.
        9,  1, seip,
        // 10, 1, heip,
        0, XLEN, mip,
    ];

    // TODO: riscv-32 support.
    Satp, "satp", 0x180, 0x00, [
        0, 44, ppn,
        44, 16, asid,
        60, 4, mode,
    ];

    // ==================================
    //            M-Mode CSR
    // ==================================
    Mstatus, "mstatus", 0x300, 0x00, [
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

    Misa, "misa", 0x301, 0x00, [
        0, 25, extension,
        -2, 2, mxl,
    ];

    Medeleg, "medeleg", 0x302, 0x00, [
        0, 1, instruction_misaligned,
        1, 1, instruction_fault,
        2, 1, illegal_instruction,
        3, 1, breakpoint,
        4, 1, load_misaligned,
        5, 1, load_fault,
        6, 1, store_misaligned,
        7, 1, store_fault,
        8, 1, user_env_call,
        9, 1, supervisor_env_call,
        // 10, 1, hypervisor_env_call,
        11, 1, machine_env_call,
        12, 1, instruction_page_fault,
        13, 1, load_page_fault,
        15, 1, store_page_fault,
        0, XLEN, medeleg,
    ];

    // see mip.
    Mideleg, "mideleg", 0x303, 0x00, [
        1, 1, ssip, // Delegate Supervisor Software Interrupt.
        5, 1, stip, // Delegate Supervisor Time     Interrupt.
        9, 1, seip, // Delegate Supervisor External Interrupt.
        0, XLEN, mideleg,
    ];

    Mie, "mie", 0x304, 0x00, [
        0,  1, usie, // User Software Interrupt Enable
        1,  1, ssie,
        2,  1, msie,
        4,  1, utie, // User Time     Interrupt Enable
        5,  1, stie,
        6,  1, mtie,
        8,  1, ueie, // User External Interrupt Enable
        9,  1, seie,
        10, 1, meie,
        0, XLEN, mip
    ];

    Mtvec, "mtvec", 0x305, 0x00, [
        0, 2, mode,
        2, XLEN - 2, base,
    ];

    Mscratch, "mscratch", 0x340, 0x00, [
        0, XLEN, mscratch,
    ];

    Mepc, "mepc", 0x341, 0x00, [
        0, XLEN, mepc,
    ];

    Mcause, "mcause", 0x342, 0x00, [
        0, XLEN - 1, exception_code,
        -1, 1, interrupt,
    ];

    Mtval, "mtval", 0x343, 0x00, [
        0, XLEN, mtval,
    ];

    Mip, "mip", 0x344, 0x00, [
        0,  1, usip, // User Software Interrupt Pending.
        1,  1, ssip,
        // 2,  1, hsip,
        3,  1, msip,
        4,  1, utip, // User Time     Interrupt Pending.
        5,  1, stip,
        // 6,  1, htip,
        7,  1, mtip,
        8,  1, ueip, // User External Interrupt Pending.
        9,  1, seip,
        // 10, 1, heip,
        11, 1, meip,
        0, XLEN, mip,
    ];

    // TODO: Below are just stub to make riscv-tests executable, not fully implemented.
    Mhartid, "mhartid", 0xF14, 0x00, [
        0, XLEN, mhartid,
    ];

    // Mnstatus, 0x744, 0x00, [
    //     0, XLEN, mnstatus,
    // ];

    // Pmpaddr0, 0x3B0, 0x00, [
    //     0, XLEN, pmpaddr0,
    // ];

    // Pmpcfg0, 0x3A0, 0x00, [
    //     0, XLEN, pmpcfg0,
    // ];
}
