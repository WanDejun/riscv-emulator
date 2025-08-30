use std::{cell::Cell, rc::Rc, u64};

/// Simple clock, clone by ref, cannot used in multi-threaded context.
#[derive(Clone)]
pub(crate) struct VirtualClockRef {
    time: Rc<Cell<u64>>,
}

impl VirtualClockRef {
    pub fn new() -> Self {
        Self {
            time: Rc::new(Cell::new(0)),
        }
    }

    pub fn set(&self, time: u64) {
        self.time.set(time);
    }

    pub fn advance(&self, delta: u64) {
        let prev = self.time.get();
        self.time.set(prev.wrapping_add(delta));
    }

    pub fn now(&self) -> u64 {
        self.time.get()
    }
}

struct ScheduledTask {
    due: u64,
    seq: u64,
    callback: Box<dyn FnMut()>,
}

impl ScheduledTask {
    fn new<F: FnMut() + 'static>(seq: u64, callback: F) -> Self {
        Self {
            due: u64::MAX,
            seq: seq,
            callback: Box::new(callback),
        }
    }
}

pub(crate) struct Timer {
    seq: u64,
    tasks: Vec<ScheduledTask>,
    vclock: VirtualClockRef,
}

impl Timer {
    pub fn new(vclock: VirtualClockRef) -> Self {
        Self {
            seq: 0,
            tasks: Vec::new(),
            vclock,
        }
    }

    /// Register a new task without setting a due, returning the sequence ID of the task.
    #[must_use]
    pub fn register<F>(&mut self, callback: F) -> u64
    where
        F: FnMut() + 'static,
    {
        let st = ScheduledTask::new(self.seq, callback);
        self.seq += 1;
        self.tasks.push(st);

        self.seq - 1
    }

    pub fn build(&mut self) {
        self.tasks.sort_unstable_by_key(|task| task.due);
    }

    /// Set the due time, use [`Timer::set_delay`] for a certain delay.
    ///
    /// NOTE: If you want to change multiply tasks, use [`Timer::guard`] instead.
    pub fn set_due(&mut self, seq: u64, new_due: u64) {
        self.guard().set_due(seq, new_due);
    }

    /// Set the due time to the current time + given delay.
    ///
    /// NOTE: If you want to change multiply tasks, use [`Timer::guard`] instead.
    pub fn set_delay(&mut self, seq: u64, delay: u64) {
        let now = self.vclock.now();
        self.set_due(seq, now.saturating_add(delay));
    }

    /// Start a guard that allows batching multiple changes without rebuilding on each change.
    /// When the returned `TimerGuard` is dropped, the timer will be rebuilt.
    pub fn guard(&mut self) -> TimerGuard<'_> {
        TimerGuard { timer: self }
    }

    /// Run all tasks whose due time is <= the timer's clock `now()`.
    pub fn tick(&mut self) {
        let now = self.vclock.now();

        self.tasks
            .iter_mut()
            .take_while(|task| task.due <= now)
            .for_each(|task| {
                (task.callback)();
                task.due = u64::MAX;
            });

        self.build();
    }

    /// Peek the next scheduled due time, if any.
    pub fn next_due(&self) -> Option<u64> {
        self.tasks.first().map(|s| s.due)
    }
}

/// RAII guard returned by [`Timer::guard()`].
pub struct TimerGuard<'a> {
    timer: &'a mut Timer,
}

impl<'a> TimerGuard<'a> {
    /// See [Timer::register].
    pub fn register<F>(&mut self, callback: F) -> u64
    where
        F: FnMut() + 'static,
    {
        self.timer.register(callback)
    }

    /// See [`Timer::set_due`].
    pub fn set_due(&mut self, seq: u64, new_due: u64) {
        self.timer
            .tasks
            .iter_mut()
            .find(|task| task.seq == seq)
            .map(|task| task.due = new_due);
    }

    /// See [`Timer::set_delay`].
    pub fn set_delay(&mut self, seq: u64, delay: u64) {
        let now = self.timer.vclock.now();
        self.set_due(seq, now.saturating_add(delay));
    }
}

impl<'a> Drop for TimerGuard<'a> {
    fn drop(&mut self) {
        self.timer.build();
    }
}
