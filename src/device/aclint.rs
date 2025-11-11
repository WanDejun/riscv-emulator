use std::{cell::UnsafeCell, rc::Rc};

use crate::{
    board::virt::{IRQLine, RiscvIRQSource},
    config::arch_config::WordType,
    device::{
        DeviceTrait, Mem, MemError, MemMappedDeviceTrait,
        config::{CLINT_BASE, CLINT_SIZE},
    },
    utils::{concat_to_u64, negative_of},
    vclock::{Timer, VirtualClockRef},
};

pub struct Clint {
    hart_num: u32,
    time_base: u64,
    timecmp_base: u64,
    time_offset: u64,
    clock: VirtualClockRef,
    timer: Rc<UnsafeCell<Timer>>,
    time_cmp: Vec<u64>,
    irq_line: Option<IRQLine>,
    timer_cb_id: u64,
}

impl Clint {
    pub fn new(
        hart_num: u32,
        mtime_base: u64,
        mtimecmp_base: u64,
        clock: VirtualClockRef,
        timer: Rc<UnsafeCell<Timer>>,
    ) -> Self {
        Self {
            hart_num,
            time_base: mtime_base,
            timecmp_base: mtimecmp_base,
            time_offset: negative_of(clock.now()),
            clock,
            timer,
            time_cmp: vec![0u64; hart_num as usize],
            irq_line: None,
            timer_cb_id: u64::MAX,
        }
    }
}

impl Clint {
    fn get_time(&mut self) -> u64 {
        self.clock.now().wrapping_add(self.time_offset)
    }

    fn handle_mtimecmp_write(&mut self, hartid: usize, value: u64) {
        self.time_cmp[hartid] = value;
        if self.time_cmp[hartid] <= self.get_time() {
            self.irq_line.as_mut().unwrap().set_irq(true);
        } else {
            unsafe { self.timer.as_mut_unchecked() }.set_due(self.timer_cb_id, value);
        }
    }
}

impl RiscvIRQSource for Clint {
    fn set_irq_line(&mut self, line: IRQLine, _id: usize) {
        self.irq_line = Some(line);
        self.timer_cb_id = unsafe { self.timer.as_mut_unchecked() }.register({
            let irq_line_ptr = self.irq_line.as_mut().unwrap() as *mut IRQLine;
            move || {
                unsafe { &mut *irq_line_ptr }.set_irq(true);
            }
        });
    }
}

impl Mem for Clint {
    fn read<T>(&mut self, addr: WordType) -> Result<T, MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        log::trace!("Clint::read: addr = {:#x}", addr);

        if self.timecmp_base <= addr && addr < self.timecmp_base + ((self.hart_num as u64) << 3) {
            // timecmp
            let hartid = ((addr - self.timecmp_base) >> 3) as usize;

            if hartid >= self.time_cmp.len() {
                return Err(MemError::LoadFault);
            }

            if (addr & 0x7) == 0 {
                return Ok(T::truncate_from(self.time_cmp[hartid]));
            } else if (addr & 0x7) == 4 {
                // timecmp_hi in RV32
                return Ok(T::truncate_from(self.time_cmp[hartid] >> 32));
            } else {
                return Err(MemError::LoadFault);
            }
        } else if addr == self.time_base {
            return Ok(T::truncate_from(self.get_time()));
        } else if addr == self.time_base + 4 {
            // time_hi for RV32
            return Ok(T::truncate_from(self.get_time() >> 32));
        }

        Err(MemError::LoadFault)
    }

    fn write<T>(&mut self, addr: WordType, data: T) -> Result<(), MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        log::trace!("Clint::write: addr = {:#x}", addr);

        if self.timecmp_base <= addr && addr < self.timecmp_base + ((self.hart_num as u64) << 3) {
            // timecmp
            let hartid = ((addr - self.timecmp_base) >> 3) as usize;

            if hartid >= self.time_cmp.len() {
                return Err(MemError::StoreFault);
            }

            if (addr & 0x7) == 0 {
                self.handle_mtimecmp_write(hartid, data.truncate_to());
            } else if (addr & 0x7) == 4 {
                let timecmp_lo = (self.time_cmp[hartid] & 0xffffffff) as u32;
                let timecmp_hi: u32 = data.truncate_to();
                self.handle_mtimecmp_write(hartid, concat_to_u64(timecmp_hi, timecmp_lo));
            }
            Ok(())
        } else if addr == self.time_base || addr == self.time_base + 4 {
            // mtime
            let curr_clocktime = self.clock.now();
            let prev_mtime = self.get_time();

            if addr == self.time_base {
                let value = if T::BITS == 32 {
                    let time_hi = (prev_mtime >> 32) as u32;
                    let time_lo: u32 = data.truncate_to();
                    concat_to_u64(time_hi, time_lo)
                } else {
                    data.truncate_to()
                };

                // To make new `mtime` value == curr_clocktime + time_offset (mod 2^64)
                self.time_offset = value.wrapping_sub(curr_clocktime);
            } else {
                // addr == self.time_base + 4, write to `mtime_hi`
                let time_lo = (prev_mtime & 0xffff_ffff) as u32;
                let time_hi: u32 = data.truncate_to();
                let value = concat_to_u64(time_hi, time_lo);
                self.time_offset = value.wrapping_sub(curr_clocktime);
            }
            Ok(())
        } else {
            Err(MemError::StoreFault)
        }
    }
}

impl DeviceTrait for Clint {
    fn sync(&mut self) {
        // Nothing to do
    }
    fn get_poll_enent(&mut self) -> Option<crate::async_poller::PollingEvent> {
        None
    }
}

impl MemMappedDeviceTrait for Clint {
    fn base() -> WordType {
        CLINT_BASE
    }
    fn size() -> WordType {
        CLINT_SIZE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_clint() -> (Clint, Rc<UnsafeCell<Timer>>) {
        let clock = VirtualClockRef::new();
        let timer = Rc::new(UnsafeCell::new(Timer::new(clock.clone())));
        let clint = Clint::new(1, 0x0200bff8, 0x02004000, clock, timer.clone());
        (clint, timer)
    }

    #[test]
    fn test_clint_creation() {
        let (clint, _timer) = create_test_clint();
        assert_eq!(clint.hart_num, 1);
        assert_eq!(clint.time_base, 0x0200bff8);
        assert_eq!(clint.timecmp_base, 0x02004000);
        assert_eq!(clint.time_cmp.len(), 1);
        assert!(clint.irq_line.is_none());
    }

    #[test]
    fn test_mtime_read_write() {
        let (mut clint, _timer) = create_test_clint();

        let initial_time: u64 = clint.read(0x0200bff8).unwrap();
        assert_eq!(initial_time, 0);

        // 测试写入 MTIME 寄存器 (32位低位)
        clint.write::<u32>(0x0200bff8, 0x12345678).unwrap();
        let time_low: u32 = clint.read(0x0200bff8).unwrap();
        assert_eq!(time_low, 0x12345678);

        // 测试写入 MTIME 寄存器 (32位高位)
        clint.write::<u32>(0x0200bffc, 0x87654321).unwrap();
        let time_high: u32 = clint.read(0x0200bffc).unwrap();
        assert_eq!(time_high, 0x87654321);

        let full_time: u64 = clint.read(0x0200bff8).unwrap();
        assert_eq!(full_time, 0x8765432112345678);
    }

    #[test]
    fn test_mtimecmp_read_write() {
        let (mut clint, _timer) = create_test_clint();

        // 测试读取 MTIMECMP 寄存器 (Hart 0)
        let initial_timecmp: u64 = clint.read(0x02004000).unwrap();
        assert_eq!(initial_timecmp, 0);

        // 测试写入 MTIMECMP 寄存器 (64位)
        clint.write::<u64>(0x02004000, 0x123456789abcdef0).unwrap();
        let timecmp: u64 = clint.read(0x02004000).unwrap();
        assert_eq!(timecmp, 0x123456789abcdef0);

        // 测试分别写入高低32位
        clint.write::<u32>(0x02004000, 0xdeadbeef).unwrap(); // 低32位
        clint.write::<u32>(0x02004004, 0xcafebabe).unwrap(); // 高32位

        let timecmp_low: u32 = clint.read(0x02004000).unwrap();
        let timecmp_high: u32 = clint.read(0x02004004).unwrap();
        assert_eq!(timecmp_low, 0xdeadbeef);
        assert_eq!(timecmp_high, 0xcafebabe);
    }

    #[test]
    fn test_invalid_address_access() {
        let (mut clint, _timer) = create_test_clint();

        let result: Result<u32, _> = clint.read(0x12345678);
        assert_eq!(result, Err(MemError::LoadFault));

        let result = clint.write::<u32>(0x12345678, 0xdeadbeef);
        assert_eq!(result, Err(MemError::StoreFault));
    }
}
