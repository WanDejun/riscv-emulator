use core::panic;
use std::ops::{Index, IndexMut};

use crate::{
    config::arch_config::WordType,
    device::MemError,
    ram_config,
    utils::{read_raw_ptr, write_raw_ptr},
};

#[repr(align(4096))]
pub struct Ram {
    data: Box<[u8]>,
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
            data: vec![0u8; ram_config::SIZE].into_boxed_slice(),
        }
    }

    pub fn insert_section(&mut self, elf_section_data: &[u8], start_addr: WordType) {
        if start_addr >= ram_config::SIZE as WordType {
            log::error!(
                "ram::insert_section out of range! start_addr = {}",
                start_addr
            );
            panic!();
        }

        let start_addr = start_addr as usize;
        elf_section_data.iter().enumerate().for_each(|(index, v)| {
            self.data[start_addr + index] = *v;
        });
    }

    // pub fn read_byte(&mut self, addr: WordType) -> u8 {
    //     Self::read::<u8>(self, addr)
    // }
    // pub fn read_word(&mut self, addr: WordType) -> u16 {
    //     Self::read::<u16>(self, addr)
    // }
    // pub fn read_dword(&mut self, addr: WordType) -> u32 {
    //     Self::read::<u32>(self, addr)
    // }
    // pub fn read_qword(&mut self, addr: WordType) -> u64 {
    //     Self::read::<u64>(self, addr)
    // }

    // pub fn write_byte(&mut self, data: u8, addr: WordType) {
    //     Self::write::<u8>(self, addr, data)
    // }
    // pub fn write_word(&mut self, data: u16, addr: WordType) {
    //     Self::write::<u16>(self, addr, data)
    // }
    // pub fn write_dword(&mut self, data: u32, addr: WordType) {
    //     Self::write::<u32>(self, addr, data)
    // }
    // pub fn write_qword(&mut self, data: u64, addr: WordType) {
    //     Self::write::<u64>(self, addr, data)
    // }
    pub fn read<T>(&self, addr: WordType) -> Result<T, MemError> {
        if addr.gt(&(ram_config::SIZE as WordType)) {
            return Err(MemError::LoadFault);
        }

        let data = unsafe { read_raw_ptr::<T>(self.data.as_ptr().add(addr as usize)) };
        if let Some(data) = data {
            Ok(data)
        } else {
            Err(MemError::LoadMisaligned)
        }
    }

    pub fn write<T>(&mut self, addr: WordType, data: T) -> Result<(), MemError> {
        if addr.gt(&(ram_config::SIZE as WordType)) {
            return Err(MemError::StoreFault);
        }

        let ret = unsafe { write_raw_ptr(self.data.as_mut_ptr().add(addr as usize), data) };
        if let Some(()) = ret {
            Ok(())
        } else {
            Err(MemError::StoreMisaligned)
        }
    }
}

// // there is nothing to do.
// impl DeviceTrait for Ram {
//     fn sync(&mut self) {}
// }

#[cfg(test)]
mod tests {
    use std::ptr::addr_of;

    use super::*;

    #[test]
    fn test_ram_new() {
        let r = Ram::new();
        // 初始化应全部为0
        for byte in r.data.into_iter() {
            assert_eq!(byte, 0);
        }
    }

    #[test]
    fn test_insert_section_and_read() {
        let mut r = Ram::new();

        // 插入一段数据，地址从 ram_config::BASE_ADDR 开始
        let base = 0x00;
        let section = [0x12u8, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0];
        r.insert_section(&section, base);

        // 验证内存中数据被正确写入
        for (i, &v) in section.iter().enumerate() {
            assert_eq!(r.data[i], v);
        }

        // 测试read_byte
        let b = r.read::<u8>(base as WordType).unwrap();
        assert_eq!(b, 0x12);

        // 测试read_word (2字节)
        let w = r.read::<u16>(base as WordType).unwrap();
        // 注意你的concat_bits顺序是从高位到低位
        // data[base|1], data[base] => 0x34 0x12 -> 0x3412
        assert_eq!(w, 0x3412);

        // 测试read_dword (4字节)
        let d = r.read::<u32>(base as WordType).unwrap();
        // data[base|3], base|2, base|1, base
        // 0x78 0x56 0x34 0x12 -> 0x78563412
        assert_eq!(d, 0x78563412);

        // 测试read_qword (8字节)
        let q = r.read::<u64>(base as WordType).unwrap();
        // data[base|7]...data[base]
        // 0xF0 0xDE 0xBC 0x9A 0x78 0x56 0x34 0x12 -> 0xF0DEBC9A78563412
        assert_eq!(q, 0xF0DEBC9A78563412);
    }

    #[test]
    #[should_panic(expected = "Result::unwrap()")]
    fn test_read_unaligned_address() {
        let r = Ram::new();
        // 这里用一个不对齐的地址试试，比如 addr & align_ilst[size_of_t] != 0 会断言失败
        r.read::<u64>(1).unwrap(); // 如果1不对齐，应该panic
    }

    #[test]
    fn test_write_byte() {
        let mut ram = Ram::new();
        ram.write::<u8>(0x00, 0xAB).unwrap();
        assert_eq!(ram.data[0], 0xAB);

        ram.write::<u16>(0x00, 0x1234).unwrap();
        assert_eq!(ram.data[0], 0x34); // little endian
        assert_eq!(ram.data[1], 0x12);

        ram.write::<u32>(0x00, 0x12345678).unwrap();
        assert_eq!(ram.data[0], 0x78);
        assert_eq!(ram.data[1], 0x56);
        assert_eq!(ram.data[2], 0x34);
        assert_eq!(ram.data[3], 0x12);

        ram.write::<u64>(0x00, 0x1122334455667788).unwrap();
        assert_eq!(ram.data[0], 0x88);
        assert_eq!(ram.data[1], 0x77);
        assert_eq!(ram.data[2], 0x66);
        assert_eq!(ram.data[3], 0x55);
        assert_eq!(ram.data[4], 0x44);
        assert_eq!(ram.data[5], 0x33);
        assert_eq!(ram.data[6], 0x22);
        assert_eq!(ram.data[7], 0x11);
    }

    #[test]
    fn test_ram_align() {
        let ram = Ram::new();
        let addr = addr_of!(ram);
        assert_eq!(addr as usize & 0xfff, 0);
    }
}
