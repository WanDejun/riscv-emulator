use crate::device::plic::ExternalInterrupt;

#[cfg(feature = "riscv64")]
use crate::device::plic::irq_line::{PlicIRQLine, PlicIRQSource};

use std::sync::{Arc, Mutex};

pub trait PollingEventTrait: Send {
    /// Poll once without blocking the caller thread.
    fn poll_nonblocking(&mut self) -> Option<ExternalInterrupt>;
}

struct PollerCore {
    events: Arc<Mutex<Vec<Box<dyn PollingEventTrait>>>>,

    #[cfg(feature = "riscv64")]
    plic_irq_line: Option<PlicIRQLine>,
}

impl PollerCore {
    fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
            #[cfg(feature = "riscv64")]
            plic_irq_line: None,
        }
    }

    fn add_event(&mut self, event: Box<dyn PollingEventTrait>) {
        self.events.lock().unwrap().push(event);
    }

    fn poll_once_collect(
        events: &Arc<Mutex<Vec<Box<dyn PollingEventTrait>>>>,
    ) -> Vec<ExternalInterrupt> {
        let mut pending = Vec::new();
        let mut guard = events.lock().unwrap();
        for event in guard.iter_mut() {
            if let Some(id) = event.poll_nonblocking() {
                pending.push(id);
            }
        }
        pending
    }

    fn dispatch_irq(&mut self, id: ExternalInterrupt) {
        if let Some(line) = self.plic_irq_line.as_mut() {
            line.set_irq(id, true);
        } else {
            log::error!("Unable to get plic_irq_line");
        }
    }

    fn dispatch_irqs(&mut self, irqs: Vec<ExternalInterrupt>) {
        for id in irqs {
            self.dispatch_irq(id);
        }
    }

    fn set_irq_line(&mut self, line: PlicIRQLine) {
        self.plic_irq_line = Some(line);
    }
}

/// DevicePoller is responsible for polling events from devices and trigger corresponding external interrupts.
///
/// - In native targets, the polling is done in a separate thread;
/// - while in web targets, the polling is done in the main thread (when `trigger_external_interrupt` is called).
#[doc(inline)]
pub use imp::DevicePoller;

#[cfg(feature = "multithreading")]
mod imp {
    use std::{thread, time::Duration};

    use crossbeam::channel::{self, Receiver, Sender};

    use super::*;

    pub enum PollerCommand {
        Exit,
    }

    pub struct DevicePoller {
        core: PollerCore,

        /// Used in polling thread to send interrupt id.
        irq_sender: Sender<ExternalInterrupt>,
        irq_receiver: Receiver<ExternalInterrupt>,

        control_sender: Sender<PollerCommand>,
        /// Used in polling thread to receive control commands.
        control_receiver: Receiver<PollerCommand>,
    }

    impl DevicePoller {
        pub fn new() -> Self {
            let (irq_sender, irq_receiver) = channel::unbounded();
            let (control_sender, control_receiver) = channel::unbounded();

            Self {
                core: PollerCore::new(),
                irq_sender,
                irq_receiver,
                control_sender,
                control_receiver,
            }
        }

        pub fn add_event(&mut self, event: Box<dyn PollingEventTrait>) {
            self.core.add_event(event);
        }

        pub fn start_polling(self) -> Self {
            let events = self.core.events.clone();
            let sender = self.irq_sender.clone();
            let control_receiver = self.control_receiver.clone();

            thread::spawn(move || {
                loop {
                    if let Ok(PollerCommand::Exit) = control_receiver.try_recv() {
                        return;
                    }

                    let pending = PollerCore::poll_once_collect(&events);
                    let triggered = !pending.is_empty();
                    for id in pending {
                        let _ = sender.send(id);
                    }

                    if !triggered {
                        thread::sleep(Duration::from_millis(10));
                    }
                }
            });

            self
        }

        pub fn stop_polling(&self) {
            let _ = self.control_sender.send(PollerCommand::Exit);
        }

        pub fn trigger_external_interrupt(&mut self) {
            while let Ok(id) = self.irq_receiver.try_recv() {
                self.core.dispatch_irq(id);
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
            self.core.set_irq_line(line);
        }
    }
}

#[cfg(not(feature = "multithreading"))]
mod imp {
    #[cfg(feature = "riscv64")]
    use super::{PlicIRQLine, PlicIRQSource};
    use super::{PollerCore, PollingEventTrait};

    pub struct DevicePoller {
        core: PollerCore,
    }

    impl DevicePoller {
        pub fn new() -> Self {
            Self {
                core: PollerCore::new(),
            }
        }

        pub fn add_event(&mut self, event: Box<dyn PollingEventTrait>) {
            self.core.add_event(event);
        }

        pub fn start_polling(self) -> Self {
            self
        }

        pub fn stop_polling(&self) {}

        pub fn trigger_external_interrupt(&mut self) {
            let pending = PollerCore::poll_once_collect(&self.core.events);
            self.core.dispatch_irqs(pending);
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
            self.core.set_irq_line(line);
        }
    }
}
