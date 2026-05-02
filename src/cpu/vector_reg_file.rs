use std::slice::{from_raw_parts, from_raw_parts_mut};

use crate::isa::riscv::vector::VLEN_BYTE;

pub struct VectorRegFile {
    reg: [[u8; VLEN_BYTE]; 32],
}

impl VectorRegFile {
    pub fn new() -> Self {
        Self {
            reg: [[0; VLEN_BYTE]; 32],
        }
    }

    pub fn read(&self, lmul: u8, idx: u8) -> Option<&[u8]> {
        self.read_as_type::<u8>(lmul, idx)
    }

    #[inline]
    pub fn read_as_type<T>(&self, lmul: u8, idx: u8) -> Option<&[T]> {
        debug_assert!(idx < 32);
        debug_assert!(lmul == 1 || lmul == 2 || lmul == 4 || lmul == 8);

        if idx % lmul != 0 {
            None
        } else {
            let base = &self.reg[idx as usize] as *const u8 as *const T;
            let slides =
                unsafe { from_raw_parts(base, VLEN_BYTE * (lmul as usize) / size_of::<T>()) };
            Some(slides)
        }
    }

    #[inline]
    pub fn get_mut<T>(&self, lmul: u8, idx: u8, seq: u8) -> Option<&mut [T]> {
        let lmul = lmul * seq;
        debug_assert!(idx < 32);
        assert!(lmul == 1 || lmul == 2 || lmul == 4 || lmul == 8);

        if idx % lmul != 0 {
            None
        } else {
            let base = &self.reg[idx as usize] as *const u8 as *mut T;
            let slides =
                unsafe { from_raw_parts_mut(base, VLEN_BYTE * (lmul as usize) / size_of::<T>()) };
            Some(slides)
        }
    }

    pub fn write<T>(&mut self, lmul: u8, idx: u8, data: &[T], seq: u8) -> Option<()>
    where
        T: Sized,
    {
        debug_assert!(idx < 32);
        debug_assert!(lmul == 1 || lmul == 2 || lmul == 4 || lmul == 8);
        let element_size = size_of::<T>();
        debug_assert!(element_size <= 8);
        if seq * lmul > 8 {
            return None;
        }

        if idx % (lmul * seq) != 0 {
            None
        } else {
            let mut input_index = 0;

            for lmul_index in 0..lmul {
                let reg_start = idx + lmul_index;
                for inner_index in (0..VLEN_BYTE).step_by(element_size) {
                    for reg_index in (reg_start..reg_start + lmul * seq).step_by(lmul as usize) {
                        let dest =
                            &mut self.reg[reg_index as usize][inner_index] as *mut _ as *mut T;
                        let src = &data[input_index] as *const _ as *const T;
                        unsafe {
                            dest.write(src.read());
                        }
                        input_index += 1
                    }
                }
            }

            Some(())
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn read_test() {
        const VLEN: usize = 128;
        let mut regfile = VectorRegFile::new();
        let mut buffer = [0u32; VLEN];
        buffer
            .iter_mut()
            .enumerate()
            .for_each(|(idx, v)| *v = idx as u32 * 2);
        regfile.write::<u32>(2, 0, &buffer, 1);
        let result = regfile.read_as_type::<u16>(1, 1).unwrap();
        // println!("{:?}", result);
        result.iter().enumerate().for_each(|(idx, v)| {
            if idx % 2 == 0 {
                assert!(*v == (8 + idx as u16))
            }
        });
    }

    #[test]
    fn write_test_without_segement() {
        const VLEN: usize = 128;
        let mut regfile = VectorRegFile::new();
        let mut buffer = [0u32; VLEN];
        buffer
            .iter_mut()
            .enumerate()
            .for_each(|(idx, v)| *v = idx as u32);

        // seg = 1
        regfile.write::<u32>(4, 0, &buffer, 1);
        assert!(regfile.read_as_type::<u32>(2, 3).is_none()); // unaligned
        let res = regfile.read_as_type::<u32>(2, 2).unwrap(); // reg_u32[8..16]
        // println!("{:?}", res);
        res.iter()
            .enumerate()
            .for_each(|(idx, v)| assert!(*v == 8 + idx as u32));
    }

    #[test]
    fn write_segement_test() {
        const VLEN: usize = 128;
        let mut regfile = VectorRegFile::new();
        let mut buffer = [0u32; VLEN];
        buffer
            .iter_mut()
            .enumerate()
            .for_each(|(idx, v)| *v = idx as u32);

        // seg = 2
        regfile.write::<u32>(2, 0, &buffer, 4);
        assert!(regfile.read_as_type::<u32>(2, 3).is_none()); // unaligned
        let res = regfile.read_as_type::<u32>(2, 2).unwrap(); // reg_u32[8..16] = buffer[4..8] ## buffer[12..16]
        // println!("{:?}", res);
        res.iter()
            .enumerate()
            .for_each(|(idx, &v)| assert!(v == idx as u32 * 4 + 1));

        assert!(regfile.write::<u32>(2, 0, &buffer, 8).is_none()); // LMUL * seq > 8
    }
}
