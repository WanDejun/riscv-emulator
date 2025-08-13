use crate::device::{DeviceTrait, Mem};

pub(crate) const POWER_OFF_CODE: u16 = 0x5555;

pub struct PowerManager {
    reg: u16,
}

impl Mem for PowerManager {
    fn read<T>(&mut self, addr: crate::config::arch_config::WordType) -> T
    where
        T: crate::utils::UnsignedInteger,
    {
        assert!(addr == 0x00);
        assert!(size_of::<T>() >= 2);
        let mut ret: T = ((self.reg >> 8) as u8).into();
        ret <<= 8;
        ret |= (self.reg as u8).into();
        ret
    }

    fn write<T>(&mut self, addr: crate::config::arch_config::WordType, data: T)
    where
        T: crate::utils::UnsignedInteger,
    {
        assert!(addr == 0x00);
        let data: u64 = data.into();
        self.reg = data as u16;
    }
}

impl DeviceTrait for PowerManager {
    fn one_shot(&mut self) {}
}

impl PowerManager {
    pub fn new() -> Self {
        Self { reg: 0 }
    }

    pub fn shut_down(&self) -> bool {
        self.reg.eq(&POWER_OFF_CODE)
    }
}
