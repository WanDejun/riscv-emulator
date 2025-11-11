// A simple timer(nano_seconds) for external interrupt testing.

#![cfg(feature = "test-device")]
use std::{
    hint::unlikely,
    mem::transmute_copy,
    time::{self, UNIX_EPOCH},
};

use crossbeam::channel::{Receiver, Sender};

use crate::{
    async_poller::PollingEventTrait,
    device::{DeviceTrait, Mem, MemError, plic::ExternalInterrupt},
    utils::check_align,
};

pub const TEST_DEVICE_INTERRUPT_ID: ExternalInterrupt = 63;

struct TestDeviceLayout {
    control_register: u32,
    interrupt_mask_register: u32,
    data_register0: u32,
    data_register1: u32,
}

impl TestDeviceLayout {
    fn new() -> Self {
        Self {
            control_register: 0,
            interrupt_mask_register: 0,
            data_register0: 0,
            data_register1: 0,
        }
    }
}

enum PollerDataPackage {
    InterruptStatus(bool),
    Data(u64),
}
pub struct TestDevicePoller {
    enable: bool,
    pre_time: u64,
    step_time: u64,
    receiver: Receiver<PollerDataPackage>,
}

impl TestDevicePoller {
    fn new(receiver: Receiver<PollerDataPackage>) -> Self {
        Self {
            enable: false,
            pre_time: 0,
            step_time: 0,
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
}

impl Mem for TestDevice {
    fn read<T>(&mut self, addr: u64) -> Result<T, crate::device::MemError>
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

    fn write<T>(&mut self, addr: u64, data: T) -> Result<(), crate::device::MemError>
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
                self.layout.interrupt_mask_register = data_u32;
                let interrupt_status = if (data_u32 & 1) != 0 { true } else { false };
                self.sender
                    .try_send(PollerDataPackage::InterruptStatus(interrupt_status))
                    .unwrap();
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
    fn get_poll_enent(&mut self) -> Option<crate::async_poller::PollingEvent> {
        let poller = TestDevicePoller::new(self.receiver.clone());
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
                    self.step_time = t;
                    let cur = time::SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .expect("time went backwards")
                        .as_nanos();
                    self.pre_time = cur as u64;
                }
                PollerDataPackage::InterruptStatus(flag) => {
                    self.enable = flag;
                }
            }
        }
        if !self.enable {
            return None;
        }

        let cur = time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time went backwards")
            .as_nanos() as u64;
        let target = self.pre_time + self.step_time;
        if cur >= target {
            self.pre_time = target;
            self.enable = false;
            Some(63)
        } else {
            None
        }
    }
}
