use std::{
    sync::{Arc, Mutex},
    thread,
};

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
}

impl AsyncPoller {
    pub fn new() -> Self {
        Self {
            enents: Arc::new(Mutex::new(vec![])),
        }
    }

    pub fn add_event(&mut self, event: PollingEvent) {
        self.enents.lock().unwrap().push(event);
    }

    pub fn start_polling(self) -> Self {
        let events = self.enents.clone();
        thread::spawn(move || {
            loop {
                // just scheduling enents by order.
                let guard = events.lock().unwrap();
                for event in guard.iter() {
                    match event {
                        PollingEvent::Uart(event) => {
                            event.poll();
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
}
