impl Drop for BackgroundExecutor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[doc(inline)]
pub use imp::BackgroundExecutor;

#[cfg(feature = "multithreading")]
mod imp {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::Duration,
    };

    type ExecTask = Box<dyn FnMut() -> bool + Send>;

    /// How long the worker sleeps after an iteration in which no task made progress.
    const IDLE_BACKOFF: Duration = Duration::from_millis(10);

    enum ExecContext {
        Running {
            poll_worker: thread::JoinHandle<Vec<ExecTask>>,
        },
        Stopped {
            poll_tasks: Vec<ExecTask>,
        },
    }

    impl Default for ExecContext {
        fn default() -> Self {
            ExecContext::Stopped { poll_tasks: vec![] }
        }
    }

    pub struct BackgroundExecutor {
        /// Cleared on shutdown; observed by the worker.
        running: Arc<AtomicBool>,

        context: ExecContext,
    }

    impl BackgroundExecutor {
        pub fn new() -> Self {
            Self {
                running: Arc::new(AtomicBool::new(true)),
                context: ExecContext::Stopped { poll_tasks: vec![] },
            }
        }

        /// Register a non-blocking task onto the shared worker thread.
        /// The task is invoked once per loop iteration and must return promptly;
        /// task should returns `true` when it made progress (keeping the loop hot) or `false` to let the loop back off.
        pub fn add_polling_task(&mut self, task: impl FnMut() -> bool + Send + 'static) {
            let ExecContext::Stopped { poll_tasks } = &mut self.context else {
                log::warn!("add polling task after executor started");
                return;
            };
            poll_tasks.push(Box::new(task));
        }

        /// Start the shared worker thread.
        pub fn start(&mut self) {
            let ExecContext::Stopped { mut poll_tasks } = std::mem::take(&mut self.context) else {
                log::warn!("executor has started");
                return;
            };

            // Start polling thread
            let running = self.running.clone();

            self.context = ExecContext::Running {
                poll_worker: thread::spawn(move || {
                    while running.load(Ordering::Relaxed) {
                        if !poll_round(&mut poll_tasks) {
                            thread::sleep(IDLE_BACKOFF);
                        }
                    }
                    poll_tasks
                }),
            };
        }

        pub fn poll_once(&mut self) {}

        /// Signal threads to stop and join the worker.
        pub fn shutdown(&mut self) {
            let ExecContext::Running { poll_worker } = std::mem::take(&mut self.context) else {
                log::warn!("try to shutdown executor when not running");
                return;
            };

            // let the worker thread exit.
            self.running.store(false, Ordering::Relaxed);

            self.context = ExecContext::Stopped {
                poll_tasks: poll_worker.join().unwrap(),
            }
        }
    }

    /// Run every polling task once; returns whether any reported progress.
    fn poll_round(poll_tasks: &mut Vec<ExecTask>) -> bool {
        let mut progressed = false;
        for task in poll_tasks.iter_mut() {
            if task() {
                progressed = true;
            }
        }
        progressed
    }
}

#[cfg(not(feature = "multithreading"))]
mod imp {
    type PollTask = Box<dyn FnMut() -> bool + Send>;

    pub struct BackgroundExecutor {
        poll_tasks: Vec<PollTask>,
    }

    impl BackgroundExecutor {
        pub fn new() -> Self {
            Self {
                poll_tasks: Vec::new(),
            }
        }

        pub fn add_polling_task(&mut self, task: impl FnMut() -> bool + Send + 'static) {
            self.poll_tasks.push(Box::new(task));
        }

        /// No worker thread to start.
        pub fn start(&mut self) {}

        /// Run every polling task once on the calling thread
        pub fn poll_once(&mut self) {
            for task in self.poll_tasks.iter_mut() {
                let _ = task();
            }
        }

        pub fn shutdown(&mut self) {}
    }
}
