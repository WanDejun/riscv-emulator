use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use crossbeam::channel::{self, Receiver, Sender};

use crate::device::plic::ExternalInterrupt;
#[cfg(feature = "riscv64")]
use crate::device::plic::irq_line::{PlicIRQLine, PlicIRQSource};

pub trait PollingEventTrait: Send {
    fn poll(&mut self) -> Option<ExternalInterrupt>;
}

pub enum PollerCommand {
    Exit,
}

pub struct DevicePoller {
    events: Arc<Mutex<Vec<Box<dyn PollingEventTrait>>>>,

    /// Used in polling thread to send interrupt id.
    irq_sender: Sender<ExternalInterrupt>,
    irq_receiver: Receiver<ExternalInterrupt>,

    control_sender: Sender<PollerCommand>,
    /// used in polling thread to receive control commands.
    control_receiver: Receiver<PollerCommand>,

    #[cfg(feature = "riscv64")]
    plic_irq_line: Option<PlicIRQLine>,
}

impl DevicePoller {
    pub fn new() -> Self {
        let (irq_sender, irq_receiver) = channel::unbounded();
        let (control_sender, control_receiver) = channel::unbounded();

        let events = Arc::new(Mutex::new(Vec::new()));

        Self {
            events,
            irq_sender,
            irq_receiver,

            control_sender,
            control_receiver,

            #[cfg(feature = "riscv64")]
            plic_irq_line: None,
        }
    }

    /// Register an epoll event. This event must implement the PollingEventTrait.
    /// If an external interrupt needs to be triggered, the interrupt number can be
    /// returned through the poll() function.
    pub fn add_event(&mut self, event: Box<dyn PollingEventTrait>) {
        self.events.lock().unwrap().push(event);
    }

    /// Start a polling thread.
    pub fn start_polling(self) -> Self {
        let events = self.events.clone();
        let sender = self.irq_sender.clone();
        let control_receiver = self.control_receiver.clone();
        thread::spawn(move || {
            loop {
                if let Ok(PollerCommand::Exit) = control_receiver.try_recv() {
                    return;
                }

                let mut guard = events.lock().unwrap();
                let mut triggered = false;
                for event in guard.iter_mut() {
                    if let Some(id) = event.poll() {
                        let _ = sender.send(id);
                        triggered = true;
                    }
                }
                drop(guard);

                // Only sleep when no event is triggered,
                // to reduce latency when events are frequent.
                if !triggered {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        });
        self
    }

    pub fn stop_polling(&self) {
        self.control_sender.send(PollerCommand::Exit).unwrap();
    }

    /// get the result of event.poll().
    pub fn trigger_external_interrupt(&mut self) {
        while let Ok(id) = self.irq_receiver.try_recv() {
            self.plic_irq_line.as_mut().unwrap().set_irq(id, true);
        }
    }
}

impl Drop for DevicePoller {
    fn drop(&mut self) {
        self.stop_polling();
    }
}

#[cfg(feature = "riscv64")]
impl PlicIRQSource for DevicePoller {
    fn set_irq_line(&mut self, line: PlicIRQLine, _id: usize) {
        self.plic_irq_line = Some(line);
    }
}
