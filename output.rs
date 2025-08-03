#![feature(prelude_import)]
#![allow(incomplete_features)]
#![feature(generic_const_exprs)]
#[prelude_import]
use std::prelude::rust_2024::*;
#[macro_use]
extern crate std;
mod config {
    #![allow(unused)]
    pub mod ram_config {
        use crate::config::arch_config::WordType;
        pub const BASE_ADDR: WordType = 0x8000_0000;
        pub const SIZE: usize = 0x8000;
    }
    pub mod arch_config {
        pub enum Arch {
            RISCV32,
            RISCV64,
        }
        #[automatically_derived]
        impl ::core::marker::StructuralPartialEq for Arch {}
        #[automatically_derived]
        impl ::core::cmp::PartialEq for Arch {
            #[inline]
            fn eq(&self, other: &Arch) -> bool {
                let __self_discr = ::core::intrinsics::discriminant_value(self);
                let __arg1_discr = ::core::intrinsics::discriminant_value(other);
                __self_discr == __arg1_discr
            }
        }
        #[automatically_derived]
        impl ::core::cmp::Eq for Arch {
            #[inline]
            #[doc(hidden)]
            #[coverage(off)]
            fn assert_receiver_is_total_eq(&self) -> () {}
        }
        #[automatically_derived]
        impl ::core::cmp::PartialOrd for Arch {
            #[inline]
            fn partial_cmp(
                &self,
                other: &Arch,
            ) -> ::core::option::Option<::core::cmp::Ordering> {
                let __self_discr = ::core::intrinsics::discriminant_value(self);
                let __arg1_discr = ::core::intrinsics::discriminant_value(other);
                ::core::cmp::PartialOrd::partial_cmp(&__self_discr, &__arg1_discr)
            }
        }
        #[automatically_derived]
        impl ::core::cmp::Ord for Arch {
            #[inline]
            fn cmp(&self, other: &Arch) -> ::core::cmp::Ordering {
                let __self_discr = ::core::intrinsics::discriminant_value(self);
                let __arg1_discr = ::core::intrinsics::discriminant_value(other);
                ::core::cmp::Ord::cmp(&__self_discr, &__arg1_discr)
            }
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for Arch {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::write_str(
                    f,
                    match self {
                        Arch::RISCV32 => "RISCV32",
                        Arch::RISCV64 => "RISCV64",
                    },
                )
            }
        }
        pub enum Endianness {
            Little,
            Big,
        }
        #[automatically_derived]
        impl ::core::fmt::Debug for Endianness {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
                ::core::fmt::Formatter::write_str(
                    f,
                    match self {
                        Endianness::Little => "Little",
                        Endianness::Big => "Big",
                    },
                )
            }
        }
        #[automatically_derived]
        impl ::core::marker::StructuralPartialEq for Endianness {}
        #[automatically_derived]
        impl ::core::cmp::PartialEq for Endianness {
            #[inline]
            fn eq(&self, other: &Endianness) -> bool {
                let __self_discr = ::core::intrinsics::discriminant_value(self);
                let __arg1_discr = ::core::intrinsics::discriminant_value(other);
                __self_discr == __arg1_discr
            }
        }
        #[automatically_derived]
        impl ::core::cmp::Eq for Endianness {
            #[inline]
            #[doc(hidden)]
            #[coverage(off)]
            fn assert_receiver_is_total_eq(&self) -> () {}
        }
        #[automatically_derived]
        impl ::core::cmp::PartialOrd for Endianness {
            #[inline]
            fn partial_cmp(
                &self,
                other: &Endianness,
            ) -> ::core::option::Option<::core::cmp::Ordering> {
                let __self_discr = ::core::intrinsics::discriminant_value(self);
                let __arg1_discr = ::core::intrinsics::discriminant_value(other);
                ::core::cmp::PartialOrd::partial_cmp(&__self_discr, &__arg1_discr)
            }
        }
        #[automatically_derived]
        impl ::core::cmp::Ord for Endianness {
            #[inline]
            fn cmp(&self, other: &Endianness) -> ::core::cmp::Ordering {
                let __self_discr = ::core::intrinsics::discriminant_value(self);
                let __arg1_discr = ::core::intrinsics::discriminant_value(other);
                ::core::cmp::Ord::cmp(&__self_discr, &__arg1_discr)
            }
        }
        pub const ARCH: crate::config::arch_config::Arch = Arch::RISCV64;
        pub type WordType = u64;
        pub const MEM_ORDER: crate::config::arch_config::Endianness = Endianness::Big;
        pub const MEM_ORDER_LIST: [usize; 8] = match Endianness::Big {
            Endianness::Little => [0x0, 0x1, 0x2, 0x3, 0x4, 0x5, 0x6, 0x7],
            Endianness::Big => [0x7, 0x6, 0x5, 0x4, 0x3, 0x2, 0x1, 0x0],
        };
        pub const REGFILE_CNT: usize = [
            "zero",
            "ra",
            "sp",
            "gp",
            "tp",
            "t0",
            "t1",
            "t2",
            "s0/fp",
            "s1",
            "a0",
            "a1",
            "a2",
            "a3",
            "a4",
            "a5",
            "a6",
            "a7",
            "s2",
            "s3",
            "s4",
            "s5",
            "s6",
            "s7",
            "s8",
            "s9",
            "s10",
            "s11",
            "t3",
            "t4",
            "t5",
            "t6",
        ]
            .len();
        pub const REG_NAME: [&str; REGFILE_CNT] = [
            "zero",
            "ra",
            "sp",
            "gp",
            "tp",
            "t0",
            "t1",
            "t2",
            "s0/fp",
            "s1",
            "a0",
            "a1",
            "a2",
            "a3",
            "a4",
            "a5",
            "a6",
            "a7",
            "s2",
            "s3",
            "s4",
            "s5",
            "s6",
            "s7",
            "s8",
            "s9",
            "s10",
            "s11",
            "t3",
            "t4",
            "t5",
            "t6",
        ];
    }
}
mod cpu {
    mod reg_file {
        use std::{fmt::Debug, ops::{Index, IndexMut}};
        use crate::config::arch_config::{REGFILE_CNT, WordType};
        pub struct RegFile {
            data: [WordType; REGFILE_CNT],
        }
        impl Index<usize> for RegFile {
            type Output = WordType;
            fn index(&self, index: usize) -> &Self::Output {
                &self.data[index]
            }
        }
        impl IndexMut<usize> for RegFile {
            fn index_mut(&mut self, index: usize) -> &mut Self::Output {
                &mut self.data[index]
            }
        }
        impl Debug for RegFile {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let byte_len = size_of::<WordType>();
                let hex_width = byte_len * 2;
                f.write_fmt(format_args!("reg_file {{\n"))?;
                for (i, val) in self.data.iter().enumerate() {
                    if i % 8 == 0 {
                        f.write_fmt(format_args!("  "))?;
                    }
                    f.write_fmt(
                        format_args!("x{0:02}: 0x{1:02$x}  ", i, val, hex_width),
                    )?;
                    if i % 8 == 7 {
                        f.write_fmt(format_args!("\n"))?;
                    }
                }
                if self.data.len() % 8 != 0 {
                    f.write_fmt(format_args!("\n"))?;
                }
                f.write_fmt(format_args!("}}"))
            }
        }
        impl RegFile {
            pub fn new() -> Self {
                Self { data: [0; REGFILE_CNT] }
            }
        }
    }
}
mod load {
    use crate::{config::arch_config::WordType, ram::Ram, ram_config::BASE_ADDR};
    #[allow(unused)]
    fn load_elf(ram: &mut Ram, elf_data: &[u8]) {
        let elf = xmas_elf::ElfFile::new(elf_data).unwrap();
        let elf_header = elf.header;
        let magic = elf_header.pt1.magic;
        match (&magic, &[0x7f, 0x45, 0x4c, 0x46]) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(
                        kind,
                        &*left_val,
                        &*right_val,
                        ::core::option::Option::Some(format_args!("invalid elf!")),
                    );
                }
            }
        };
        let ph_count = elf_header.pt2.ph_count();
        for i in 0..ph_count {
            let ph = elf.program_header(i).unwrap();
            if ph.get_type().unwrap() == xmas_elf::program::Type::Load {
                let start_addr = (ph.virtual_addr() as usize) as WordType;
                let end_addr = ((ph.virtual_addr() + ph.mem_size()) as usize)
                    as WordType;
                ram.insert_section(
                    &elf
                        .input[ph.offset()
                        as usize..(ph.offset() + ph.file_size()) as usize],
                    start_addr,
                );
            }
        }
    }
    #[allow(unused)]
    fn load_bin(ram: &mut Ram, raw_data: &[u8]) {
        ram.insert_section(raw_data, BASE_ADDR);
    }
}
mod ram {
    use core::panic;
    use std::ops::{Index, IndexMut};
    use crate::{
        config::arch_config::WordType, ram_config::{self, BASE_ADDR},
        utils::{read_raw_ptr, write_raw_ptr},
    };
    #[repr(align(4096))]
    pub struct Ram {
        data: [u8; ram_config::SIZE],
    }
    impl Index<usize> for Ram {
        type Output = u8;
        fn index(&self, index: usize) -> &Self::Output {
            &(self.data[index])
        }
    }
    impl IndexMut<usize> for Ram {
        fn index_mut(&mut self, index: usize) -> &mut Self::Output {
            &mut (self.data[index])
        }
    }
    impl Ram {
        /// TODO: use random init for better debug.
        pub fn new() -> Self {
            Self {
                data: [0u8; ram_config::SIZE],
            }
        }
        pub fn insert_section(&mut self, elf_section_data: &[u8], start_addr: WordType) {
            if start_addr < ram_config::BASE_ADDR
                || start_addr - ram_config::BASE_ADDR >= ram_config::SIZE as WordType
            {
                {
                    {
                        let lvl = ::log::Level::Error;
                        if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                            ::log::__private_api::log(
                                { ::log::__private_api::GlobalLogger },
                                format_args!(
                                    "ram::insert_section out of range! start_addr = {0}",
                                    start_addr,
                                ),
                                lvl,
                                &(
                                    "riscv_emulater::ram",
                                    "riscv_emulater::ram",
                                    ::log::__private_api::loc(),
                                ),
                                (),
                            );
                        }
                    }
                };
                {
                    #[cold]
                    #[track_caller]
                    #[inline(never)]
                    const fn panic_cold_explicit() -> ! {
                        ::core::panicking::panic_explicit()
                    }
                    panic_cold_explicit();
                };
            }
            let start_addr = (start_addr - BASE_ADDR) as usize;
            elf_section_data
                .iter()
                .enumerate()
                .for_each(|(index, v)| {
                    self.data[start_addr + index] = *v;
                });
        }
        pub fn read<T>(&self, addr: WordType) -> T {
            unsafe {
                read_raw_ptr::<T>(self.data.as_ptr().add((addr - BASE_ADDR) as usize))
            }
        }
        #[allow(unused)]
        pub fn read_byte(&self, addr: WordType) -> u8 {
            Self::read::<u8>(self, addr)
        }
        #[allow(unused)]
        pub fn read_word(&self, addr: WordType) -> u16 {
            Self::read::<u16>(self, addr)
        }
        #[allow(unused)]
        pub fn read_dword(&self, addr: WordType) -> u32 {
            Self::read::<u32>(self, addr)
        }
        #[allow(unused)]
        pub fn read_qword(&self, addr: WordType) -> u64 {
            Self::read::<u64>(self, addr)
        }
        pub fn write<T>(&mut self, data: T, addr: WordType) {
            unsafe {
                write_raw_ptr(
                    self.data.as_mut_ptr().add((addr - BASE_ADDR) as usize),
                    data,
                );
            }
        }
        #[allow(unused)]
        pub fn write_byte(&mut self, data: u8, addr: WordType) {
            Self::write::<u8>(self, data, addr)
        }
        #[allow(unused)]
        pub fn write_word(&mut self, data: u16, addr: WordType) {
            Self::write::<u16>(self, data, addr)
        }
        #[allow(unused)]
        pub fn write_dword(&mut self, data: u32, addr: WordType) {
            Self::write::<u32>(self, data, addr)
        }
        #[allow(unused)]
        pub fn write_qword(&mut self, data: u64, addr: WordType) {
            Self::write::<u64>(self, data, addr)
        }
    }
}
mod device {
    use crate::config::arch_config::WordType;
    pub trait DeviceTrait {
        fn write<T>(device: &mut T, addr: usize, data: WordType)
        where
            T: DeviceTrait;
        fn read<T>(device: &mut T, addr: usize, data: WordType)
        where
            T: DeviceTrait;
    }
}
mod logging {
    use log::{self, Level, LevelFilter, Log, Metadata, Record};
    struct SimpleLogger;
    impl Log for SimpleLogger {
        fn enabled(&self, _metadata: &Metadata) -> bool {
            true
        }
        fn log(&self, record: &Record) {
            if !self.enabled(record.metadata()) {
                return;
            }
            let color = match record.level() {
                Level::Error => 31,
                Level::Warn => 93,
                Level::Info => 34,
                Level::Debug => 32,
                Level::Trace => 90,
            };
            {
                ::std::io::_print(
                    format_args!(
                        "\u{1b}[{0}m[{1:>5}] {2}\u{1b}[0m\n",
                        color,
                        record.level(),
                        record.args(),
                    ),
                );
            };
        }
        fn flush(&self) {}
    }
    pub fn init() {
        static LOGGER: SimpleLogger = SimpleLogger;
        log::set_logger(&LOGGER).unwrap();
        log::set_max_level(
            match ::core::option::Option::None::<&'static str> {
                Some("ERROR") => LevelFilter::Error,
                Some("WARN") => LevelFilter::Warn,
                Some("INFO") => LevelFilter::Info,
                Some("DEBUG") => LevelFilter::Debug,
                Some("TRACE") => LevelFilter::Trace,
                _ => LevelFilter::Info,
            },
        );
    }
}
mod utils {
    use std::usize;
    use crate::config::arch_config::WordType;
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
        if !((addr as WordType) & ALIGN_ILST[size_of_t] == 0) {
            {
                ::core::panicking::panic_fmt(
                    format_args!(
                        "read_word -> addr: {0}, is not aligned!",
                        addr as usize,
                    ),
                );
            }
        }
        let ptr = addr as *const T;
        unsafe { ptr.read() }
    }
    pub unsafe fn write_raw_ptr<T>(addr: *mut u8, data: T) {
        let size_of_t: usize = size_of::<T>();
        if !((addr as WordType) & ALIGN_ILST[size_of_t] == 0) {
            {
                ::core::panicking::panic_fmt(
                    format_args!(
                        "read_word -> addr: {0}, is not aligned!",
                        addr as usize,
                    ),
                );
            }
        }
        let ptr = addr as *mut T;
        unsafe { ptr.write(data) }
    }
}
pub use config::ram_config;
fn init() {
    logging::init();
}
fn main() {
    init();
    let x = [(1, 2, 3)];
    {
        {
            let lvl = ::log::Level::Error;
            if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                ::log::__private_api::log(
                    { ::log::__private_api::GlobalLogger },
                    format_args!("[Error] "),
                    lvl,
                    &("riscv_emulater", "riscv_emulater", ::log::__private_api::loc()),
                    (),
                );
            }
        }
    };
    {
        {
            let lvl = ::log::Level::Warn;
            if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                ::log::__private_api::log(
                    { ::log::__private_api::GlobalLogger },
                    format_args!("[Warn]   "),
                    lvl,
                    &("riscv_emulater", "riscv_emulater", ::log::__private_api::loc()),
                    (),
                );
            }
        }
    };
    {
        {
            let lvl = ::log::Level::Info;
            if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                ::log::__private_api::log(
                    { ::log::__private_api::GlobalLogger },
                    format_args!("[info]   "),
                    lvl,
                    &("riscv_emulater", "riscv_emulater", ::log::__private_api::loc()),
                    (),
                );
            }
        }
    };
    {
        {
            let lvl = ::log::Level::Debug;
            if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                ::log::__private_api::log(
                    { ::log::__private_api::GlobalLogger },
                    format_args!("[debug] "),
                    lvl,
                    &("riscv_emulater", "riscv_emulater", ::log::__private_api::loc()),
                    (),
                );
            }
        }
    };
    {
        {
            let lvl = ::log::Level::Trace;
            if lvl <= ::log::STATIC_MAX_LEVEL && lvl <= ::log::max_level() {
                ::log::__private_api::log(
                    { ::log::__private_api::GlobalLogger },
                    format_args!("[trace] "),
                    lvl,
                    &("riscv_emulater", "riscv_emulater", ::log::__private_api::loc()),
                    (),
                );
            }
        }
    };
}
