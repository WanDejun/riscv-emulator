use std::slice::{from_raw_parts, from_raw_parts_mut};

#[repr(u8)]
pub(super) enum FixedPointRoundingMode {
    RoundToNearestUp = 0x00,
    RoundToNearestEven = 0x01,
    RoundDown = 0x02,
    RoundToOdd = 0x03,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub(super) enum Vlmul {
    M1 = 0,
    M2 = 1,
    M4 = 2,
    M8 = 3,
    Mf8 = 5,
    Mf4 = 6,
    Mf2 = 7,
}

impl From<u8> for Vlmul {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::M1,
            1 => Self::M2,
            2 => Self::M4,
            3 => Self::M8,
            5 => Self::Mf8,
            6 => Self::Mf4,
            7 => Self::Mf2,
            _ => panic!(),
        }
    }
}

impl Into<u8> for Vlmul {
    fn into(self) -> u8 {
        self as u8
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub(super) enum Vsew {
    E8 = 0,
    E16 = 1,
    E32 = 2,
    E64 = 3,
}

impl From<u8> for Vsew {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::E8,
            1 => Self::E16,
            2 => Self::E32,
            3 => Self::E64,
            _ => panic!(),
        }
    }
}

impl Into<u8> for Vsew {
    fn into(self) -> u8 {
        self as u8
    }
}

pub(super) struct VectorConfig {
    pub(super) lmul: Vlmul,
    pub(super) sew: Vsew,
    pub(super) tail_agnostic: bool,
    pub(super) mask_agnostic: bool,
    pub(super) fixed_point_accrued_aturation_flag: bool,
    pub(super) fixed_point_rounding_mode: FixedPointRoundingMode,
    pub(super) vl: u16, // [0, 10240 * 8 / 8]
}

impl VectorConfig {
    pub(super) fn new() -> Self {
        Self {
            lmul: Vlmul::M1,
            sew: Vsew::E8,
            tail_agnostic: false,
            mask_agnostic: false,
            fixed_point_accrued_aturation_flag: false,
            fixed_point_rounding_mode: FixedPointRoundingMode::RoundToNearestUp,
            vl: 0,
        }
    }
}

pub struct VGFRef<'a> {
    value: &'a [u8],
    sew: usize,
}

impl<'a> VGFRef<'a> {
    pub fn new(sew: u8, val: &'a [u8]) -> Self {
        Self {
            value: val,
            sew: sew as usize,
        }
    }

    pub fn get<T: Sized>(&self, index: usize) -> T
    where
        T: Clone,
    {
        assert!(self.sew + 1 == size_of::<T>());
        unsafe {
            let p = self.value.as_ptr() as *const T;
            let s = from_raw_parts(p, self.value.len() >> self.sew);
            s[index].clone()
        }
    }
}

pub struct VGFRefMut<'a> {
    value: &'a mut [u8],
    sew: usize,
}

impl<'a> VGFRefMut<'a> {
    pub fn new(sew: u8, val: &'a mut [u8]) -> Self {
        Self {
            value: val,
            sew: sew as usize,
        }
    }

    pub fn get<T: Sized>(&self, index: usize) -> T
    where
        T: Clone,
    {
        assert!(self.sew + 1 == size_of::<T>());
        unsafe {
            let p = self.value.as_ptr() as *const T;
            let s = from_raw_parts(p, self.value.len() >> self.sew);
            s[index].clone()
        }
    }

    pub fn set<T: Sized>(&self, index: usize, value: T)
    where
        T: Clone,
    {
        assert!(self.sew + 1 == size_of::<T>());
        unsafe {
            let p = self.value.as_ptr() as *mut T;
            let s = from_raw_parts_mut(p, self.value.len() >> self.sew);
            s[index] = value
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn vector_type_test() {
        let value: Vec<u8> = (0..128).map(|i| if i % 2 == 0 { 0 } else { i }).collect();
        let v = VGFRef::new(1, value.as_slice());
        assert_eq!(v.get::<u16>(3), (3 * 2 + 1) << 8);

        let mut value_mut: Vec<u8> = (0..128).map(|i| if i % 2 == 0 { 0 } else { i }).collect();
        let v = VGFRefMut::new(1, value_mut.as_mut_slice());
        assert_eq!(v.get::<u16>(3), (3 * 2 + 1) << 8);
        v.set::<u16>(3, 3);
        assert_eq!(v.get::<u16>(3), 3);
    }

    #[test]
    #[should_panic(expected = "assertion failed: self.sew + 1 == size_of::<T>()")]
    fn vector_type_test_unequal_sew() {
        let value: Vec<u8> = (0..128).map(|i| if i % 2 == 0 { 0 } else { i }).collect();
        let v = VGFRef::new(1, value.as_slice());
        assert_eq!(v.get::<u32>(3), (3 * size_of::<u32>() as u32 + 1) << 8)
    }
}
