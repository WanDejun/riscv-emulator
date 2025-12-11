// A simple timer(microseconds) for external interrupt testing.

#![cfg(feature = "test-device")]
use std::{
    hint::unlikely,
    mem::transmute_copy,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::{self, Duration, SystemTime},
};

use crossbeam::channel::{Receiver, Sender};

use crate::{
    async_poller::PollingEventTrait,
    device::{DeviceTrait, MemError, plic::ExternalInterrupt},
    utils::check_align,
};

pub const TEST_DEVICE_INTERRUPT_ID: ExternalInterrupt = 63;

struct TestDeviceLayout {
    control_register: u32,
    interrupt_mask_register: Arc<AtomicU32>,
    data_register0: u32,
    data_register1: u32,
}

impl TestDeviceLayout {
    fn new() -> Self {
        Self {
            control_register: 0,
            interrupt_mask_register: Arc::new(AtomicU32::new(0)),
            data_register0: 0,
            data_register1: 0,
        }
    }
}

enum PollerDataPackage {
    // InterruptStatus(u32),
    Data(u64),
}
pub struct TestDevicePoller {
    interrupt_mask_register: Arc<AtomicU32>,
    pre_time: SystemTime,
    step_time: Duration,
    receiver: Receiver<PollerDataPackage>,
}

impl TestDevicePoller {
    fn new(receiver: Receiver<PollerDataPackage>, imr: Arc<AtomicU32>) -> Self {
        Self {
            interrupt_mask_register: imr,
            pre_time: SystemTime::now(),
            step_time: Duration::from_micros(0),
            receiver,
        }
    }
}

pub(crate) struct TestDevice {
    layout: TestDeviceLayout,
    sender: Sender<PollerDataPackage>,
    receiver: Receiver<PollerDataPackage>,
}

impl TestDevice {
    pub fn new() -> Self {
        let (sender, receiver) = crossbeam::channel::unbounded();
        Self {
            layout: TestDeviceLayout::new(),
            sender,
            receiver,
        }
    }

    fn get_data64(&self) -> u64 {
        (self.layout.data_register1 as u64) << 32 | (self.layout.data_register0 as u64)
    }

    fn read_impl<T>(&mut self, addr: u64) -> Result<T, crate::device::MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        if unlikely(!check_align::<u32>(addr)) {
            return Err(crate::device::MemError::LoadMisaligned);
        }

        let data = match addr {
            0x00 => unsafe { transmute_copy(&self.layout.control_register) },
            0x04 => unsafe { transmute_copy(&self.layout.interrupt_mask_register) },
            0x08 => unsafe { transmute_copy(&self.layout.data_register0) },
            0x0c => unsafe { transmute_copy(&self.layout.data_register1) },
            _ => return Err(MemError::LoadFault),
        };
        return Ok(data);
    }

    fn write_impl<T>(&mut self, addr: u64, data: T) -> Result<(), crate::device::MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        if unlikely(!check_align::<u32>(addr)) {
            return Err(crate::device::MemError::StoreMisaligned);
        }

        let data_u64 = data.into();
        let data_u32 = data_u64 as u32;

        match addr {
            0x00 => self.layout.control_register = data_u32,
            0x04 => {
                self.layout
                    .interrupt_mask_register
                    .store(data_u32, Ordering::Release);
            }
            0x08 => {
                self.layout.data_register0 = data_u32;
                self.sender
                    .try_send(PollerDataPackage::Data(self.get_data64()))
                    .unwrap();
            }
            0x0c => {
                self.layout.data_register0 = data_u32;
                self.sender
                    .try_send(PollerDataPackage::Data(self.get_data64()))
                    .unwrap();
            }
            _ => return Err(MemError::StoreFault),
        };
        return Ok(());
    }
}

impl DeviceTrait for TestDevice {
    dispatch_read_write! { read_impl, write_impl }

    fn get_poll_enent(&mut self) -> Option<crate::async_poller::PollingEvent> {
        let poller = TestDevicePoller::new(
            self.receiver.clone(),
            self.layout.interrupt_mask_register.clone(),
        );
        Some(crate::async_poller::PollingEvent::TestDevice(poller))
    }
    fn sync(&mut self) {
        // nothing to do.
    }
}

impl PollingEventTrait for TestDevicePoller {
    fn poll(&mut self) -> Option<super::plic::ExternalInterrupt> {
        while let Ok(v) = self.receiver.try_recv() {
            match v {
                PollerDataPackage::Data(t) => {
                    self.step_time = Duration::from_micros(t);
                    self.pre_time = time::SystemTime::now();
                }
            }
        }
        if (self.interrupt_mask_register.load(Ordering::Acquire) & 1) == 0 {
            return None;
        }

        let cur = time::SystemTime::now();
        if cur.duration_since(self.pre_time).unwrap() > self.step_time {
            self.pre_time = cur;
            // trigger only one time -> use for debug.
            // self.interrupt_mask_register
            //     .fetch_and(!0x1, Ordering::Release);
            Some(63)
        } else {
            None
        }
    }
}
