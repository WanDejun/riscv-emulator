use std::{
    sync::{Arc, Mutex},
    thread,
};

use crossbeam::channel::{self, Receiver, Sender};

#[cfg(feature = "riscv64")]
use crate::device::plic::irq_line::{PlicIRQLine, PlicIRQSource};
#[cfg(feature = "test-device")]
use crate::device::test_device::TestDevicePoller;
use crate::device::{fast_uart::virtual_io::TerminalIO, plic::ExternalInterrupt};

pub trait PollingEventTrait {
    fn poll(&mut self) -> Option<ExternalInterrupt>;
}

pub enum AsyncPollerCommand {
    Exit,
}

pub enum PollingEvent {
    Uart(TerminalIO),
    #[cfg(feature = "test-device")]
    TestDevice(TestDevicePoller),
    Control(Receiver<AsyncPollerCommand>),
}

pub struct AsyncPoller {
    enents: Arc<Mutex<Vec<PollingEvent>>>,
    irq_sender: Sender<ExternalInterrupt>, // used in polling thread to send interrupt id.
    irq_receiver: Receiver<ExternalInterrupt>,

    control_sender: Sender<AsyncPollerCommand>,
    control_receiver: Receiver<AsyncPollerCommand>, // used in polling thread to receive control commands.

    #[cfg(feature = "riscv64")]
    plic_irq_line: Option<PlicIRQLine>,
}

impl AsyncPoller {
    pub fn new() -> Self {
        let (irq_sender, irq_receiver) = channel::unbounded();
        let (control_sender, control_receiver) = channel::unbounded();
        let enents = Arc::new(Mutex::new(vec![PollingEvent::Control(
            control_receiver.clone(),
        )]));
        Self {
            enents,
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
    pub fn add_event(&mut self, event: PollingEvent) {
        self.enents.lock().unwrap().push(event);
    }

    /// Start a polling thread.
    pub fn start_polling(self) -> Self {
        let events = self.enents.clone();
        let sender = self.irq_sender.clone();
        thread::spawn(move || {
            loop {
                // just scheduling enents by order.
                let mut guard = events.lock().unwrap();
                for event in guard.iter_mut() {
                    match event {
                        PollingEvent::Uart(event) => {
                            if let Some(id) = event.poll() {
                                sender.send(id).unwrap();
                            }
                        }
                        #[cfg(feature = "test-device")]
                        PollingEvent::TestDevice(poller) => {
                            if let Some(id) = poller.poll() {
                                sender.send(id).unwrap();
                            }
                        }
                        PollingEvent::Control(receiver) => {
                            while let Ok(v) = receiver.try_recv() {
                                match v {
                                    AsyncPollerCommand::Exit => {
                                        return; // exit current thread.
                                    }
                                }
                            }
                        }
                    }
                }
                drop(guard);
            }
        });
        self
    }

    pub fn exit_polling(&self) {
        self.control_sender.send(AsyncPollerCommand::Exit).unwrap();
    }

    /// get the result of enent.poll().
    pub fn trigger_external_interrupt(&mut self) {
        while let Ok(id) = self.irq_receiver.try_recv() {
            self.plic_irq_line.as_mut().unwrap().set_irq(id, true);
        }
    }
}

impl Drop for AsyncPoller {
    fn drop(&mut self) {
        self.exit_polling();
    }
}

#[cfg(feature = "riscv64")]
impl PlicIRQSource for AsyncPoller {
    fn set_irq_line(&mut self, line: PlicIRQLine, _id: usize) {
        self.plic_irq_line = Some(line);
    }
}
