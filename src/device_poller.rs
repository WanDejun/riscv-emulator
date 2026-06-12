use crate::device::plic::ExternalInterrupt;
use crossbeam::channel::{Receiver, Sender};

#[cfg(feature = "riscv64")]
use crate::device::plic::irq_line::{PlicIRQLine, PlicIRQSource};

use std::sync::{Arc, Mutex};

pub trait PollingEventTrait: Send {
    /// Poll once without blocking the caller thread.
    fn poll_nonblocking(&mut self) -> Option<ExternalInterrupt>;
}

pub trait PollingFn: FnMut() -> Option<ExternalInterrupt> {}

impl<F: FnMut() -> Option<ExternalInterrupt>> PollingFn for F {}

pub struct PollingFnWrapper<F>
where
    F: PollingFn + Send,
{
    f: F,
}

impl<F: PollingFn + Send> PollingFnWrapper<F> {
    pub fn new(f: F) -> Self {
        Self { f }
    }
}

impl<F: PollingFn + Send> PollingEventTrait for PollingFnWrapper<F> {
    fn poll_nonblocking(&mut self) -> Option<ExternalInterrupt> {
        (self.f)()
    }
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

    // TODO: Consider to stop returning interrupt here, let every have a Sender<ExternalInterrupt>.
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
    use crossbeam::channel;
    use std::{thread, time::Duration};

    use super::*;

    pub enum PollerCommand {
        Exit,
    }

    // TODO: Current DevicePoller is in charge of: PLIC IRQ and running background task,
    // split them into two modules, and change these wired namings.
    pub struct DevicePoller {
        core: PollerCore,

        /// Used in polling thread to send interrupt id.
        irq_sender: Sender<ExternalInterrupt>,
        irq_receiver: Receiver<ExternalInterrupt>,

        control_sender: Sender<PollerCommand>,
        /// Used in polling thread to receive control commands.
        control_receiver: Receiver<PollerCommand>,

        thread_handle: Option<thread::JoinHandle<()>>,
    }

    impl DevicePoller {
        pub fn new(
            plic_irq_tx: Sender<ExternalInterrupt>,
            plic_irq_rx: Receiver<ExternalInterrupt>,
        ) -> Self {
            let (control_sender, control_receiver) = channel::unbounded();

            Self {
                core: PollerCore::new(),
                irq_sender: plic_irq_tx,
                irq_receiver: plic_irq_rx,
                control_sender,
                control_receiver,
                thread_handle: None,
            }
        }

        pub fn add_event(&mut self, event: Box<dyn PollingEventTrait>) {
            self.core.add_event(event);
        }

        pub fn start_polling(mut self) -> Self {
            let events = self.core.events.clone();
            let sender = self.irq_sender.clone();
            let control_receiver = self.control_receiver.clone();

            self.thread_handle = Some(thread::spawn(move || {
                loop {
                    if let Ok(PollerCommand::Exit) = control_receiver.try_recv() {
                        log::trace!("exiting poller thread");
                        PollerCore::poll_once_collect(&events);
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
            }));

            self
        }

        pub fn stop_polling(&mut self) {
            log::trace!("stop polling");
            let _ = self.control_sender.send(PollerCommand::Exit);
            self.thread_handle
                .take()
                .expect("poller thread never started")
                .join()
                .unwrap();
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

    impl PlicIRQSource for DevicePoller {
        fn set_irq_line(&mut self, line: PlicIRQLine, _id: usize) {
            self.core.set_irq_line(line);
        }
    }
}

#[cfg(not(feature = "multithreading"))]
mod imp {
    use super::*;

    pub struct DevicePoller {
        core: PollerCore,
        plic_irq_rx: Receiver<ExternalInterrupt>,
    }

    impl DevicePoller {
        pub fn new(
            _plic_irq_tx: Sender<ExternalInterrupt>,
            plic_irq_rx: Receiver<ExternalInterrupt>,
        ) -> Self {
            Self {
                core: PollerCore::new(),
                plic_irq_rx,
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
            let mut pending = PollerCore::poll_once_collect(&self.core.events);
            while let Ok(id) = self.plic_irq_rx.try_recv() {
                pending.push(id);
            }
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
