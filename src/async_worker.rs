/// A non-blocking unit of device work executed by the shared background executor.
pub trait AsyncWorker: Send {
    /// Run one iteration and return whether useful work was performed.
    fn async_task(&mut self) -> bool;
}

/// Collects device workers while the board is being built.
///
/// Once board construction is complete, [`Self::into_polling_task`] moves all
/// workers into the background executor. Workers cannot be registered after
/// that point.
pub struct AsyncWorkerManager {
    workers: Vec<Box<dyn AsyncWorker>>,
}

impl AsyncWorkerManager {
    pub fn new() -> Self {
        Self {
            workers: Vec::new(),
        }
    }

    pub fn add_worker(&mut self, worker: Box<dyn AsyncWorker>) {
        self.workers.push(worker);
    }

    pub fn into_polling_task(mut self) -> impl FnMut() -> bool + Send + 'static {
        move || {
            let mut made_progress = false;
            for worker in &mut self.workers {
                made_progress |= worker.async_task();
            }
            made_progress
        }
    }
}

impl Default for AsyncWorkerManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    struct CountingWorker {
        calls: Arc<AtomicUsize>,
        reports_progress: bool,
    }

    impl AsyncWorker for CountingWorker {
        fn async_task(&mut self) -> bool {
            self.calls.fetch_add(1, Ordering::Relaxed);
            self.reports_progress
        }
    }

    #[test]
    fn polling_task_runs_every_registered_worker() {
        let first_calls = Arc::new(AtomicUsize::new(0));
        let second_calls = Arc::new(AtomicUsize::new(0));
        let mut manager = AsyncWorkerManager::new();
        manager.add_worker(Box::new(CountingWorker {
            calls: first_calls.clone(),
            reports_progress: false,
        }));
        manager.add_worker(Box::new(CountingWorker {
            calls: second_calls.clone(),
            reports_progress: true,
        }));

        let mut polling_task = manager.into_polling_task();

        assert!(polling_task());
        assert_eq!(first_calls.load(Ordering::Relaxed), 1);
        assert_eq!(second_calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn polling_task_reports_idle_when_no_worker_makes_progress() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut manager = AsyncWorkerManager::new();
        manager.add_worker(Box::new(CountingWorker {
            calls,
            reports_progress: false,
        }));

        let mut polling_task = manager.into_polling_task();

        assert!(!polling_task());
    }
}
