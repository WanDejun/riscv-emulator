use std::{
    sync::{Arc, Mutex},
    thread,
};

use crossbeam::channel::{self, Receiver, Sender};

#[cfg(feature = "riscv64")]
use crate::device::plic::irq_line::{PlicIRQLine, PlicIRQSource};
use crate::device::{fast_uart::virtual_io::TerminalIO, plic::ExternalInterrupt};

pub trait PollingEventTrait {
    fn poll(&self) -> Option<ExternalInterrupt>;
}

pub enum PollingEvent {
    Uart(TerminalIO),
    Stop,
}

pub struct AsyncPoller {
    enents: Arc<Mutex<Vec<PollingEvent>>>,
    sender: Sender<ExternalInterrupt>,
    receiver: Receiver<ExternalInterrupt>,

    #[cfg(feature = "riscv64")]
    plic_irq_line: Option<PlicIRQLine>,
}

impl AsyncPoller {
    pub fn new() -> Self {
        let (sender, receiver) = channel::unbounded();
        Self {
            enents: Arc::new(Mutex::new(vec![])),
            sender,
            receiver,
            plic_irq_line: None,
        }
    }

    /// Register an epoll event. This event must implement the PollingEventTrait.
    /// If an external interrupt needs to be triggered, the interrupt number can be
    /// returned through the poll() function.
    pub fn add_event(&mut self, event: PollingEvent) {
        self.enents.lock().unwrap().push(event);
    }

    /// Start a polling thread.
    pub fn start_polling(self) -> Self {
        let events = self.enents.clone();
        let sender = self.sender.clone();
        thread::spawn(move || {
            loop {
                // just scheduling enents by order.
                let guard = events.lock().unwrap();
                for event in guard.iter() {
                    match event {
                        PollingEvent::Uart(event) => {
                            if let Some(id) = event.poll() {
                                sender.send(id).unwrap();
                            }
                        }
                        PollingEvent::Stop => {
                            return; // exit current thread.
                        }
                    }
                }
                drop(guard);
            }
        });
        self
    }

    /// get the result of enent.poll().
    pub fn trigger_external_interrupt(&mut self) {
        while let Ok(id) = self.receiver.recv() {
            self.plic_irq_line.as_mut().unwrap().set_irq(id, true);
        }
    }
}

#[cfg(feature = "riscv64")]
impl PlicIRQSource for AsyncPoller {
    fn set_irq_line(&mut self, line: PlicIRQLine, _id: usize) {
        self.plic_irq_line = Some(line);
    }
}
