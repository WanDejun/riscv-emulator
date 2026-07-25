// A simple timer(microseconds) for external interrupt testing.

#![cfg(feature = "test-device")]
use std::{
    hint::unlikely,
    mem::transmute_copy,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

use crossbeam::channel::{Receiver, Sender};

use crate::{
    async_worker::AsyncWorker,
    config::arch_config::WordType,
    device::{
        DeviceTrait, MemError, MemMappedDeviceTrait, PlicDeviceHandler,
        config::{TEST_DEVICE_BASE, TEST_DEVICE_SIZE},
        plic::{
            PeriphIrqId,
            irq_line::{PlicIRQLine, PlicIRQSource},
        },
    },
    utils::check_align,
};

pub const TEST_DEVICE_INTERRUPT_ID: PeriphIrqId = 63;

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

enum WorkerCommand {
    // InterruptStatus(u32),
    Data {
        interval_ms: u64,
        configured_at: Instant,
    },
}
pub struct TestDeviceWorker {
    interrupt_mask_register: Arc<AtomicU32>,
    pre_time: Instant,
    step_time: Duration,
    receiver: Receiver<WorkerCommand>,
    irq_pending: Arc<AtomicBool>,
}

impl TestDeviceWorker {
    fn new(
        receiver: Receiver<WorkerCommand>,
        imr: Arc<AtomicU32>,
        irq_pending: Arc<AtomicBool>,
    ) -> Self {
        Self {
            interrupt_mask_register: imr,
            pre_time: Instant::now(),
            step_time: Duration::from_micros(0),
            receiver,
            irq_pending,
        }
    }
}

pub(crate) struct TestDevice {
    layout: TestDeviceLayout,
    sender: Sender<WorkerCommand>,
    receiver: Option<Receiver<WorkerCommand>>,
    irq_pending: Arc<AtomicBool>,
    plic_irq_line: Option<PlicIRQLine>,
}

impl TestDevice {
    pub fn new() -> Self {
        let (sender, receiver) = crossbeam::channel::unbounded();
        Self {
            layout: TestDeviceLayout::new(),
            sender,
            receiver: Some(receiver),
            irq_pending: Arc::new(AtomicBool::new(false)),
            plic_irq_line: None,
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

        let data_u32 = data.truncate_to();

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
                    .try_send(WorkerCommand::Data {
                        interval_ms: self.get_data64(),
                        configured_at: Instant::now(),
                    })
                    .unwrap();
            }
            0x0c => {
                self.layout.data_register1 = data_u32;
                self.sender
                    .try_send(WorkerCommand::Data {
                        interval_ms: self.get_data64(),
                        configured_at: Instant::now(),
                    })
                    .unwrap();
            }
            _ => return Err(MemError::StoreFault),
        };
        return Ok(());
    }
}

impl DeviceTrait for TestDevice {
    dispatch_read_write! { read_impl, write_impl }

    fn get_async_worker(&mut self) -> Option<Box<dyn AsyncWorker>> {
        let worker = TestDeviceWorker::new(
            self.receiver
                .take()
                .expect("test device async worker can only be registered once"),
            self.layout.interrupt_mask_register.clone(),
            self.irq_pending.clone(),
        );
        Some(Box::new(worker))
    }
    fn sync(&mut self) {
        // nothing to do.
    }
}

impl MemMappedDeviceTrait for TestDevice {
    fn base() -> WordType {
        TEST_DEVICE_BASE
    }
    fn size() -> WordType {
        TEST_DEVICE_SIZE
    }
}

impl PlicIRQSource for TestDevice {
    fn set_irq_line(
        &mut self,
        target: *mut dyn super::plic::irq_line::PlicIRQHandler,
        interrupt_id: PeriphIrqId,
    ) {
        let handler = Box::new(PlicTestDeviceHandler {
            irq_pending: self.irq_pending.clone(),
        });
        let line = PlicIRQLine::new(target, Some(handler), interrupt_id);
        self.plic_irq_line = Some(line);
    }
}

impl AsyncWorker for TestDeviceWorker {
    fn async_task(&mut self) -> bool {
        let mut made_progress = false;
        while let Ok(v) = self.receiver.try_recv() {
            made_progress = true;
            match v {
                WorkerCommand::Data {
                    interval_ms,
                    configured_at,
                } => {
                    self.step_time = Duration::from_millis(interval_ms);
                    self.pre_time = configured_at;
                }
            }
        }
        let cur = Instant::now();

        if (self.interrupt_mask_register.load(Ordering::Acquire) & 1) == 0 {
            self.pre_time = cur;
        }

        if cur.duration_since(self.pre_time) > self.step_time {
            println!("interrupt id: {}.", TEST_DEVICE_INTERRUPT_ID);
            self.pre_time = cur;
            // trigger only one time -> use for debug.
            // self.interrupt_mask_register
            //     .fetch_and(!0x1, Ordering::Release);

            self.irq_pending.store(true, Ordering::Release);
            made_progress = true;
        }
        made_progress
    }
}

struct PlicTestDeviceHandler {
    irq_pending: Arc<AtomicBool>,
}

impl PlicDeviceHandler for PlicTestDeviceHandler {
    fn irq_level(&self) -> bool {
        self.irq_pending.load(Ordering::Acquire)
    }
}
