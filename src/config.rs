#![allow(unused)]

pub mod ram_config {
    use crate::config::arch_config::WordType;
    pub const BASE_ADDR: WordType = 0x8000_0000;
    pub const DEFAULT_PC_VALUE: WordType = BASE_ADDR;

    pub const SIZE: usize = 0x8000000;
}

pub mod arch_config {
    use crate::gen_name_list;
    use crate::gen_reg_name_list;

    #[derive(PartialEq, Eq, PartialOrd, Ord, Debug)]
    pub enum Arch {
        RISCV32,
        RISCV64,
    }

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub enum Endianness {
        Little,
        Big,
    }

    macro_rules! mem_order_list {
        ($endian: path) => {
            match $endian {
                Endianness::Little => [0x0, 0x1, 0x2, 0x3, 0x4, 0x5, 0x6, 0x7],
                Endianness::Big => [0x7, 0x6, 0x5, 0x4, 0x3, 0x2, 0x1, 0x0],
            }
        };
    }

    macro_rules! arch_config {
        (
            $(
                @item
                $feature:literal => {
                    arch: $arch:path,
                    word: $word:ty,
                    signed_word: $signed_word:ty,
                    endian: $endian:path,
                    reg_name: $reg_name: expr,
                }
            ),* $(,)?
        ) => {
            $(
                #[cfg(feature = $feature)]
                pub const ARCH: $crate::config::arch_config::Arch = $arch;

                #[cfg(feature = $feature)]
                pub type WordType = $word;

                #[cfg(feature = $feature)]
                pub type SignedWordType = $signed_word;

                #[cfg(feature = $feature)]
                pub const MEM_ORDER: $crate::config::arch_config::Endianness = $endian;

                #[cfg(feature = $feature)]
                pub const MEM_ORDER_LIST: [usize; 8] = mem_order_list!($endian);

                #[cfg(feature = $feature)]
                pub const REGFILE_CNT: usize = $reg_name.len();

                #[cfg(feature = $feature)]
                pub const REG_NAME: [&str; REGFILE_CNT] = $reg_name;
            )*
        };
    }

    arch_config! {
        @item "riscv32" => {
            arch: Arch::RISCV32,
            word: u32,
            signed_word: i32,
            endian: Endianness::Big,
            reg_name: gen_reg_name_list!(   "zero";     "ra";           "sp";       "gp";
                                            "tp";       "t", 0, 2;      "s0/fp";    "s1";
                                            "a", 0, 7;  "s", 2, 11;     "t", 3, 6),
        },
        @item "riscv64" => {
            arch: Arch::RISCV64,
            word: u64,
            signed_word: i64,
            endian: Endianness::Big,
            reg_name: gen_reg_name_list!(   "zero";     "ra";   "sp";   "gp";
                                            "tp";       "t", 0, 2;      "s0/fp";    "s1";
                                            "a", 0, 7;  "s", 2, 11;     "t", 3, 6),
        }
    }
}
