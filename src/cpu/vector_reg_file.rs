use std::slice::from_raw_parts;

pub struct VectorRegFile<const VLEN: usize>
where
    [(); VLEN >> 3]:,
{
    reg: [[u8; VLEN >> 3]; 32],
}

impl<const VLEN: usize> VectorRegFile<VLEN>
where
    [(); VLEN >> 3]:,
{
    pub fn new() -> Self {
        Self {
            reg: [[0; VLEN >> 3]; 32],
        }
    }

    pub fn read<const LMUL: usize>(&self, idx: usize) -> Option<&[u8]> {
        self.read_as_type::<LMUL, u8>(idx)
    }

    #[inline]
    pub fn read_as_type<const LMUL: usize, T>(&self, idx: usize) -> Option<&[T]> {
        debug_assert!(idx < 32);
        debug_assert!(LMUL <= 7);

        if idx % LMUL != 0 {
            None
        } else {
            let base = &self.reg[idx] as *const u8 as *const T;
            let slides = unsafe { from_raw_parts(base, (VLEN >> 3) * LMUL / size_of::<T>()) };
            Some(slides)
        }
    }

    pub fn write<const LMUL: usize, T>(&mut self, idx: usize, data: &[T], seq: usize) -> Option<()>
    where
        T: Sized,
    {
        debug_assert!(idx < 32);
        debug_assert!(LMUL <= 7);
        let element_size = size_of::<T>();
        debug_assert!(element_size <= 8);
        if seq * LMUL > 8 {
            return None;
        }

        if idx % (LMUL * seq) != 0 {
            None
        } else {
            let mut input_index = 0;

            for lmul_index in 0..LMUL {
                let reg_start = idx + lmul_index;
                for inner_index in (0..(VLEN >> 3)).step_by(element_size) {
                    for reg_index in (reg_start..reg_start + LMUL * seq).step_by(LMUL) {
                        let dest = &mut self.reg[reg_index][inner_index] as *mut _ as *mut T;
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
        let mut regfile = VectorRegFile::<VLEN>::new();
        let mut buffer = [0u32; VLEN];
        buffer
            .iter_mut()
            .enumerate()
            .for_each(|(idx, v)| *v = idx as u32 * 2);
        regfile.write::<2, u32>(0, &buffer, 1);
        let result = regfile.read_as_type::<1, u16>(1).unwrap();
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
        let mut regfile = VectorRegFile::<VLEN>::new();
        let mut buffer = [0u32; VLEN];
        buffer
            .iter_mut()
            .enumerate()
            .for_each(|(idx, v)| *v = idx as u32);

        // seg = 1
        regfile.write::<4, u32>(0, &buffer, 1);
        assert!(regfile.read_as_type::<2, u32>(3).is_none()); // unaligned
        let res = regfile.read_as_type::<2, u32>(2).unwrap(); // reg_u32[8..16]
        // println!("{:?}", res);
        res.iter()
            .enumerate()
            .for_each(|(idx, v)| assert!(*v == 8 + idx as u32));
    }

    #[test]
    fn write_segement_test() {
        const VLEN: usize = 128;
        let mut regfile = VectorRegFile::<VLEN>::new();
        let mut buffer = [0u32; VLEN];
        buffer
            .iter_mut()
            .enumerate()
            .for_each(|(idx, v)| *v = idx as u32);

        // seg = 2
        regfile.write::<2, u32>(0, &buffer, 4);
        assert!(regfile.read_as_type::<2, u32>(3).is_none()); // unaligned
        let res = regfile.read_as_type::<2, u32>(2).unwrap(); // reg_u32[8..16] = buffer[4..8] ## buffer[12..16]
        // println!("{:?}", res);
        res.iter()
            .enumerate()
            .for_each(|(idx, &v)| assert!(v == idx as u32 * 4 + 1));

        assert!(regfile.write::<2, u32>(0, &buffer, 8).is_none()); // LMUL * seq > 8
    }
}
