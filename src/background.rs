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
        sync::{Arc, Condvar, Mutex},
        thread,
        time::Duration,
    };

    type ExecTask = Box<dyn FnMut() -> bool + Send>;

    /// How long the worker sleeps after an iteration in which no task made progress.
    const IDLE_BACKOFF: Duration = Duration::from_millis(5);

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

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ThreadState {
        Running,
        Pausing,
        Paused,
        Stopping,
        Stopped,
    }

    pub struct BackgroundExecutor {
        state_cvar: Arc<(Mutex<ThreadState>, Condvar)>,
        context: ExecContext,
    }

    impl BackgroundExecutor {
        pub fn new() -> Self {
            Self {
                state_cvar: Arc::new((Mutex::new(ThreadState::Stopped), Condvar::new())),
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

            {
                let (state, _cvar) = &*self.state_cvar;
                *state.lock().unwrap() = ThreadState::Running;
            }

            // Start polling thread
            let state_cvar = self.state_cvar.clone();

            self.context = ExecContext::Running {
                poll_worker: thread::spawn(move || {
                    let (lock, cvar) = &*state_cvar;

                    loop {
                        let mut state = lock.lock().unwrap();
                        match *state {
                            ThreadState::Running => {
                                drop(state);
                                if poll_round(&mut poll_tasks) {
                                    continue;
                                }

                                // sleep when make no progess
                                let mut state = lock.lock().unwrap();
                                if *state == ThreadState::Running {
                                    state = cvar.wait_timeout(state, IDLE_BACKOFF).unwrap().0;
                                }
                            }
                            ThreadState::Pausing | ThreadState::Paused => {
                                // do one more time to ensure uart output queue empty etc.
                                poll_round(&mut poll_tasks);

                                *state = ThreadState::Paused;
                                cvar.notify_one();

                                while *state == ThreadState::Paused {
                                    state = cvar.wait(state).unwrap();
                                }
                            }
                            ThreadState::Stopping => {
                                *state = ThreadState::Stopped;
                                return poll_tasks;
                            }
                            ThreadState::Stopped => {
                                unreachable!(
                                    "background thread is executing while its state is stopped"
                                );
                            }
                        }
                    }
                }),
            };
        }

        pub fn poll_if_single_thread_mode(&mut self) {}

        pub fn pause_and_wait(&mut self) {
            let (lock, cvar) = &*self.state_cvar;
            let mut state = lock.lock().unwrap();

            if *state == ThreadState::Stopped {
                log::error!(
                    "trying to pause background thread while it is stopped or never started"
                );
                panic!();
            }

            // avoid dead lock that both main thread and backgroud thread is waiting.
            if *state != ThreadState::Running {
                cvar.notify_one();
            }

            *state = ThreadState::Pausing;

            while *state != ThreadState::Paused {
                state = cvar.wait(state).unwrap();
            }
        }

        pub fn resume(&mut self) {
            let (lock, cvar) = &*self.state_cvar;
            let mut state = lock.lock().unwrap();

            *state = ThreadState::Running;
            cvar.notify_one();
        }

        /// Background thread will poll again before actual exited.
        pub fn shutdown(&mut self) {
            let ExecContext::Running { poll_worker } = std::mem::take(&mut self.context) else {
                log::warn!("try to shutdown executor when not running");
                return;
            };

            // Pauses first to force poll again before exit.
            self.pause_and_wait();

            {
                let (lock, cvar) = &*self.state_cvar;
                let mut state = lock.lock().unwrap();
                // avoid dead lock because background thread is paused (waiting)
                cvar.notify_one();
                *state = ThreadState::Stopping;
            }

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
        pub fn poll_if_single_thread_mode(&mut self) {
            for task in self.poll_tasks.iter_mut() {
                let _ = task();
            }
        }

        pub fn shutdown(&mut self) {}

        pub fn pause_and_wait(&mut self) {}

        pub fn resume(&mut self) {}
    }
}
