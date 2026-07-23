use crate::device::plic::ExternalInterrupt;
use crossbeam::channel::{Receiver, Sender};

#[cfg(feature = "riscv64")]
use crate::device::plic::irq_line::{PlicIRQLine, PlicIRQSource};

use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlicIRQState {
    pub interrupt_id: ExternalInterrupt,
    pub level: bool,
}

impl PlicIRQState {
    pub fn new(interrupt_id: ExternalInterrupt, level: bool) -> Self {
        Self {
            interrupt_id,
            level,
        }
    }
}

pub trait PollingEventTrait: Send {
    /// Poll once without blocking and return the current absolute PLIC IRQ line state.
    ///
    /// `level` is the line level at the time of this sample. Implementations
    /// must not convert it into a pulse or suppress unchanged levels; the
    /// poller owns transition detection.
    fn poll_nonblocking(&mut self) -> PlicIRQState;
}

pub trait PollingFn: FnMut() -> PlicIRQState {}

impl<F: FnMut() -> PlicIRQState> PollingFn for F {}

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
    fn poll_nonblocking(&mut self) -> PlicIRQState {
        (self.f)()
    }
}

struct PollingEventEntry {
    event: Box<dyn PollingEventTrait>,
    previous: Option<PlicIRQState>,
}

// TODO: `PollerCore` is unnecessary after `BackgroundExecutor` has been extracted.
struct PollerCore {
    events: Arc<Mutex<Vec<PollingEventEntry>>>,

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
        self.events.lock().unwrap().push(PollingEventEntry {
            event,
            previous: None,
        });
    }

    /// Sample each IRQ line and return only changes from its previously observed level.
    ///
    /// The returned states carry the new absolute level. Suppressing stable
    /// samples here prevents unchanged device levels from flooding the channel.
    fn poll_once_collect(events: &Arc<Mutex<Vec<PollingEventEntry>>>) -> Vec<PlicIRQState> {
        let mut changes = Vec::new();
        let mut guard = events.lock().unwrap();
        for entry in guard.iter_mut() {
            let current = entry.event.poll_nonblocking();

            match entry.previous {
                None => {
                    if current.level {
                        changes.push(current);
                    }
                }
                Some(previous) if previous.interrupt_id == current.interrupt_id => {
                    if previous.level != current.level {
                        changes.push(current);
                    }
                }
                Some(previous) => {
                    if previous.level {
                        changes.push(PlicIRQState::new(previous.interrupt_id, false));
                    }
                    if current.level {
                        changes.push(current);
                    }
                }
            }

            entry.previous = Some(current);
        }
        changes
    }

    fn dispatch_irq(&mut self, id: ExternalInterrupt, level: bool) {
        if let Some(line) = self.plic_irq_line.as_mut() {
            line.set_irq(id, level);
        } else {
            log::error!("Unable to get plic_irq_line");
        }
    }

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

    /// Level transitions produced by `poll_once_collect`, sent from the polling
    /// task and received on the main thread.
    irq_sender: Sender<PlicIRQState>,
    irq_receiver: Receiver<PlicIRQState>,
}

const MAX_IRQ_CHANGES_PER_BATCH: usize = 256;

impl DevicePoller {
    pub fn new(plic_irq_tx: Sender<PlicIRQState>, plic_irq_rx: Receiver<PlicIRQState>) -> Self {
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
            let changes = PollerCore::poll_once_collect(&events);
            let triggered = !changes.is_empty();
            for change in changes {
                let _ = sender.send(change);
            }
            triggered
        }
    }

    /// Drain the interrupts produced by the polling task and dispatch them to the PLIC. Call on the
    /// main thread after
    /// [`BackgroundExecutor::poll_once`](crate::background::BackgroundExecutor::poll_once).
    pub fn trigger_external_interrupt(&mut self) {
        let queued = self.irq_receiver.len();
        if queued > MAX_IRQ_CHANGES_PER_BATCH {
            log::warn!(
                "PLIC IRQ change queue contains {} entries; processing {} this batch",
                queued,
                MAX_IRQ_CHANGES_PER_BATCH
            );
        }

        for _ in 0..queued.min(MAX_IRQ_CHANGES_PER_BATCH) {
            let Ok(change) = self.irq_receiver.try_recv() else {
                break;
            };
            self.core.dispatch_irq(change.interrupt_id, change.level);
        }
    }
}

#[cfg(feature = "riscv64")]
impl PlicIRQSource for DevicePoller {
    fn set_irq_line(&mut self, line: PlicIRQLine, _id: ExternalInterrupt) {
        self.core.set_irq_line(line);
    }
}

#[cfg(test)]
mod test {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    #[test]
    fn stable_irq_level_only_produces_transitions() {
        let level = Arc::new(AtomicBool::new(false));
        let poll_level = level.clone();
        let mut core = PollerCore::new();
        core.add_event(Box::new(PollingFnWrapper::new(move || {
            PlicIRQState::new(10, poll_level.load(Ordering::Acquire))
        })));

        assert!(PollerCore::poll_once_collect(&core.events).is_empty());

        level.store(true, Ordering::Release);
        assert_eq!(
            PollerCore::poll_once_collect(&core.events),
            vec![PlicIRQState::new(10, true)]
        );
        assert!(PollerCore::poll_once_collect(&core.events).is_empty());

        level.store(false, Ordering::Release);
        assert_eq!(
            PollerCore::poll_once_collect(&core.events),
            vec![PlicIRQState::new(10, false)]
        );
        assert!(PollerCore::poll_once_collect(&core.events).is_empty());
    }

    #[test]
    fn changing_irq_id_deasserts_the_previous_source_first() {
        let interrupt_id = Arc::new(std::sync::atomic::AtomicU32::new(10));
        let poll_interrupt_id = interrupt_id.clone();
        let mut core = PollerCore::new();
        core.add_event(Box::new(PollingFnWrapper::new(move || {
            PlicIRQState::new(poll_interrupt_id.load(Ordering::Acquire), true)
        })));

        assert_eq!(
            PollerCore::poll_once_collect(&core.events),
            vec![PlicIRQState::new(10, true)]
        );

        interrupt_id.store(11, Ordering::Release);
        assert_eq!(
            PollerCore::poll_once_collect(&core.events),
            vec![PlicIRQState::new(10, false), PlicIRQState::new(11, true)]
        );
    }
}
