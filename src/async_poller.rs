use std::{
    sync::{Arc, Mutex},
    thread,
};

use crossbeam::channel::{self, Receiver, Sender};

use crate::device::fast_uart::virtual_io::TerminalIO;

pub trait PollingEventTrait {
    fn poll(&self) -> Option<InterryptID>;
}

pub type InterryptID = u8;

pub enum PollingEvent {
    Uart(TerminalIO),
    Stop,
}

pub struct AsyncPoller {
    enents: Arc<Mutex<Vec<PollingEvent>>>,
    sender: Sender<InterryptID>,
    receiver: Receiver<InterryptID>,
}

impl AsyncPoller {
    pub fn new() -> Self {
        let (sender, receiver) = channel::unbounded();
        Self {
            enents: Arc::new(Mutex::new(vec![])),
            sender,
            receiver,
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
    pub fn get_poller_results(&self) -> Vec<InterryptID> {
        let mut interrupts = vec![];
        while let Ok(id) = self.receiver.recv() {
            interrupts.push(id);
        }

        interrupts
    }
}
