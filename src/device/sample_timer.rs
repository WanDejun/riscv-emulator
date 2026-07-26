// A simple millisecond timer used to exercise external interrupts.

#![cfg(feature = "test-device")]
use std::{
    hint::unlikely,
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
        config::{SAMPLE_TIMER_BASE, SAMPLE_TIMER_SIZE},
        plic::{
            PeriphIrqId,
            irq_line::{PlicIRQLine, PlicIRQSource},
        },
    },
    utils::check_align,
};

pub const SAMPLE_TIMER_INTERRUPT_ID: PeriphIrqId = 63;
const CONTROL_RESET: u32 = 1 << 0;

struct SampleTimerLayout {
    control_register: u32,
    interrupt_mask_register: Arc<AtomicU32>,
    data_register0: u32,
    data_register1: u32,
}

impl SampleTimerLayout {
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
    Data {
        interval_ms: u64,
        configured_at: Instant,
    },
    Reset {
        reset_at: Instant,
    },
}
pub struct SampleTimerWorker {
    interrupt_mask_register: Arc<AtomicU32>,
    pre_time: Instant,
    step_time: Duration,
    receiver: Receiver<WorkerCommand>,
    irq_pending: Arc<AtomicBool>,
}

impl SampleTimerWorker {
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

pub(crate) struct SampleTimerDevice {
    layout: SampleTimerLayout,
    sender: Sender<WorkerCommand>,
    receiver: Option<Receiver<WorkerCommand>>,
    irq_pending: Arc<AtomicBool>,
    plic_irq_line: Option<PlicIRQLine>,
}

impl SampleTimerDevice {
    pub fn new() -> Self {
        let (sender, receiver) = crossbeam::channel::unbounded();
        Self {
            layout: SampleTimerLayout::new(),
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
            0x00 => self.layout.control_register,
            0x04 => self.layout.interrupt_mask_register.load(Ordering::Acquire),
            0x08 => self.layout.data_register0,
            0x0c => self.layout.data_register1,
            _ => return Err(MemError::LoadFault),
        };
        Ok(T::truncate_from(data))
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
            0x00 => {
                self.layout.control_register = data_u32;
                if data_u32 & CONTROL_RESET != 0 {
                    // Deassert synchronously so a following PLIC completion cannot
                    // observe the stale interrupt level before the worker runs.
                    self.irq_pending.store(false, Ordering::Release);
                    self.sender
                        .try_send(WorkerCommand::Reset {
                            reset_at: Instant::now(),
                        })
                        .unwrap();
                }
            }
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

impl DeviceTrait for SampleTimerDevice {
    dispatch_read_write! { read_impl, write_impl }

    fn get_async_worker(&mut self) -> Option<Box<dyn AsyncWorker>> {
        let worker = SampleTimerWorker::new(
            self.receiver
                .take()
                .expect("sample timer async worker can only be registered once"),
            self.layout.interrupt_mask_register.clone(),
            self.irq_pending.clone(),
        );
        Some(Box::new(worker))
    }
    fn sync(&mut self) {
        // nothing to do.
    }
}

impl MemMappedDeviceTrait for SampleTimerDevice {
    fn base() -> WordType {
        SAMPLE_TIMER_BASE
    }
    fn size() -> WordType {
        SAMPLE_TIMER_SIZE
    }
}

impl PlicIRQSource for SampleTimerDevice {
    fn set_irq_line(
        &mut self,
        target: *mut dyn super::plic::irq_line::PlicIRQHandler,
        interrupt_id: PeriphIrqId,
    ) {
        let handler = Box::new(PlicSampleTimerHandler {
            irq_pending: self.irq_pending.clone(),
        });
        let line = PlicIRQLine::new(target, Some(handler), interrupt_id);
        self.plic_irq_line = Some(line);
    }
}

impl AsyncWorker for SampleTimerWorker {
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
                WorkerCommand::Reset { reset_at } => {
                    self.irq_pending.store(false, Ordering::Release);
                    self.pre_time = reset_at;
                }
            }
        }
        let cur = Instant::now();

        if (self.interrupt_mask_register.load(Ordering::Acquire) & 1) == 0 {
            self.pre_time = cur;
            return made_progress;
        }

        if !self.irq_pending.load(Ordering::Acquire)
            && cur.duration_since(self.pre_time) >= self.step_time
        {
            log::trace!("interrupt id: {}.", SAMPLE_TIMER_INTERRUPT_ID);
            self.irq_pending.store(true, Ordering::Release);
            made_progress = true;
        }
        made_progress
    }
}

struct PlicSampleTimerHandler {
    irq_pending: Arc<AtomicBool>,
}

impl PlicDeviceHandler for PlicSampleTimerHandler {
    fn irq_level(&self) -> bool {
        self.irq_pending.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reset_rearms_timer_for_another_interrupt() {
        let (sender, receiver) = crossbeam::channel::unbounded();
        let interrupt_mask_register = Arc::new(AtomicU32::new(1));
        let irq_pending = Arc::new(AtomicBool::new(false));
        let mut worker =
            SampleTimerWorker::new(receiver, interrupt_mask_register, irq_pending.clone());
        let interval = Duration::from_millis(10);

        sender
            .send(WorkerCommand::Data {
                interval_ms: interval.as_millis() as u64,
                configured_at: Instant::now() - interval,
            })
            .unwrap();
        assert!(worker.async_task());
        assert!(irq_pending.load(Ordering::Acquire));

        worker.pre_time = Instant::now() - interval;
        assert!(!worker.async_task());

        let reset_at = Instant::now();
        sender.send(WorkerCommand::Reset { reset_at }).unwrap();
        assert!(worker.async_task());
        assert!(!irq_pending.load(Ordering::Acquire));
        assert_eq!(worker.pre_time, reset_at);

        worker.pre_time = Instant::now() - interval;
        assert!(worker.async_task());
        assert!(irq_pending.load(Ordering::Acquire));
    }

    #[test]
    fn control_bit_zero_does_not_clear_pending_interrupt() {
        let mut device = SampleTimerDevice::new();
        device.irq_pending.store(true, Ordering::Release);

        device.write_u32(0, !CONTROL_RESET).unwrap();

        assert!(device.irq_pending.load(Ordering::Acquire));
    }

    #[test]
    fn control_bit_one_clears_pending_interrupt_immediately() {
        let mut device = SampleTimerDevice::new();
        device.irq_pending.store(true, Ordering::Release);

        device.write_u32(0, CONTROL_RESET).unwrap();

        assert!(!device.irq_pending.load(Ordering::Acquire));
        assert!(matches!(
            device.receiver.as_ref().unwrap().try_recv(),
            Ok(WorkerCommand::Reset { .. })
        ));
    }
}
