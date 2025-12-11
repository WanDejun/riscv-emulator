use crate::device::{
    DeviceTrait, MemError, MemMappedDeviceTrait,
    config::{POWER_MANAGER_BASE, POWER_MANAGER_SIZE},
};
use std::sync::atomic::AtomicU16;

pub(crate) const POWER_OFF_CODE: u16 = 0x5555;
pub static POWER_STATUS: AtomicU16 = AtomicU16::new(0);

pub struct PowerManager {
    reg: u16,
}

impl PowerManager {
    fn read_impl<T>(&mut self, addr: crate::config::arch_config::WordType) -> Result<T, MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        debug_assert!(addr == 0x00);
        debug_assert!(size_of::<T>() >= 2);
        let mut ret: T = ((self.reg >> 8) as u8).into();
        ret <<= 8;
        ret |= (self.reg as u8).into();
        Ok(ret)
    }

    fn write_impl<T>(
        &mut self,
        _addr: crate::config::arch_config::WordType,
        data: T,
    ) -> Result<(), MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        debug_assert!(_addr == 0x00);
        let data: u64 = data.into();
        self.reg = data as u16;

        if self.reg == POWER_OFF_CODE {
            POWER_STATUS.store(0x5555, std::sync::atomic::Ordering::Release);
        }
        Ok(())
    }
}

impl DeviceTrait for PowerManager {
    dispatch_read_write! { read_impl, write_impl }

    fn sync(&mut self) {}
    fn get_poll_enent(&mut self) -> Option<crate::async_poller::PollingEvent> {
        None
    }
}

impl MemMappedDeviceTrait for PowerManager {
    fn base() -> crate::config::arch_config::WordType {
        POWER_MANAGER_BASE
    }

    fn size() -> crate::config::arch_config::WordType {
        POWER_MANAGER_SIZE
    }
}

impl PowerManager {
    pub fn new() -> Self {
        Self { reg: 0 }
    }

    pub fn shut_down(&self) -> bool {
        self.reg.eq(&POWER_OFF_CODE)
    }
}
