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

    #[cfg(feature = "riscv64")]
    fn dispatch_irq(&mut self, id: ExternalInterrupt) {
        if let Some(line) = self.plic_irq_line.as_mut() {
            line.set_irq(id, true);
        } else {
            log::error!("Unable to get plic_irq_line");
        }
    }

    #[cfg(feature = "riscv64")]
    fn set_irq_line(&mut self, line: PlicIRQLine) {
        self.plic_irq_line = Some(line);
    }
}

/// Groups the device poll events and ferries the [`ExternalInterrupt`]s they produce to the PLIC.
///
/// `DevicePoller` no longer owns a thread. The polling is driven by a
/// [`BackgroundExecutor`](crate::background::BackgroundExecutor): register [`Self::poll_task`] on
/// it, then drain the produced interrupts on the main thread via
/// [`Self::trigger_external_interrupt`].
///
/// - With `multithreading`, the executor runs the task on its worker thread and the interrupts
///   arrive over the channel asynchronously.
/// - Without it, the executor runs the task inline on `poll_once`, just before the drain.
pub struct DevicePoller {
    core: PollerCore,

    /// Sent from the polling task (any thread), received on the main thread.
    irq_sender: Sender<ExternalInterrupt>,
    irq_receiver: Receiver<ExternalInterrupt>,
}

impl DevicePoller {
    pub fn new(
        plic_irq_tx: Sender<ExternalInterrupt>,
        plic_irq_rx: Receiver<ExternalInterrupt>,
    ) -> Self {
        Self {
            core: PollerCore::new(),
            irq_sender: plic_irq_tx,
            irq_receiver: plic_irq_rx,
        }
    }

    pub fn add_event(&mut self, event: Box<dyn PollingEventTrait>) {
        self.core.add_event(event);
    }

    /// Build the polling task to register on a
    /// [`BackgroundExecutor`](crate::background::BackgroundExecutor). It polls every registered
    /// event once and forwards any produced interrupts to the main thread, returning `true` when at
    /// least one fired so the executor keeps its loop hot.
    pub fn poll_task(&self) -> impl FnMut() -> bool + Send + 'static {
        let events = self.core.events.clone();
        let sender = self.irq_sender.clone();
        move || {
            let pending = PollerCore::poll_once_collect(&events);
            let triggered = !pending.is_empty();
            for id in pending {
                let _ = sender.send(id);
            }
            triggered
        }
    }

    /// Drain the interrupts produced by the polling task and dispatch them to the PLIC. Call on the
    /// main thread after
    /// [`BackgroundExecutor::poll_once`](crate::background::BackgroundExecutor::poll_once).
    pub fn trigger_external_interrupt(&mut self) {
        while let Ok(_id) = self.irq_receiver.try_recv() {
            #[cfg(feature = "riscv64")]
            self.core.dispatch_irq(_id);
        }
    }
}

#[cfg(feature = "riscv64")]
impl PlicIRQSource for DevicePoller {
    fn set_irq_line(&mut self, line: PlicIRQLine, _id: usize) {
        self.core.set_irq_line(line);
    }
}
