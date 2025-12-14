use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use lazy_static::lazy_static;
use std::sync::{Arc, Condvar, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CliThreadState {
    Running,
    Pausing,
    Paused,
}

/// A coordinator to pause or resume the interaction between [`CliUart`] and the terminal.
/// It is implemented as a global singleton, accessible via [`CliCoordinator::global()`].
/// This also ensures the correct status of raw mode.
///
/// [`CliUart`]: crate::device::cli_uart::CliUart
/// [`spawn_io_thread`]: crate::device::cli_uart::spawn_io_thread
#[derive(Clone)]
pub struct CliCoordinator {
    state: Arc<(Mutex<CliThreadState>, Condvar)>,
}

lazy_static! {
    static ref INSTANCE: CliCoordinator = CliCoordinator::new();
}

impl CliCoordinator {
    pub fn new() -> Self {
        Self {
            state: Arc::new((Mutex::new(CliThreadState::Running), Condvar::new())),
        }
    }

    pub fn global() -> &'static CliCoordinator {
        &INSTANCE
    }

    pub fn pause_uart_without_wait(&self) {
        let (lock, _cvar) = &*self.state;

        {
            let mut s = lock.lock().unwrap();
            *s = CliThreadState::Pausing;
        }
    }

    pub fn pause_uart(&self) {
        let (lock, cvar) = &*self.state;

        {
            let mut s = lock.lock().unwrap();
            *s = CliThreadState::Pausing;
            // FIXME: If the uart thread is dead, this may deadlock.
            while *s != CliThreadState::Paused {
                s = cvar.wait(s).unwrap();
            }
        }

        log::trace!("Uart cli loop paused");

        disable_raw_mode().unwrap();
        log::trace!("Raw mode disabled");
    }

    pub fn resume_uart(&self) {
        let (lock, cvar) = &*self.state;
        {
            let mut s = lock.lock().unwrap();
            *s = CliThreadState::Running;
        }
        cvar.notify_all();

        log::trace!("Uart cli loop resumed");
    }

    pub fn confirm_pause_and_wait(&self) {
        let (lock, cvar) = &*self.state;
        {
            let mut s = lock.lock().unwrap();

            if *s == CliThreadState::Running {
                return;
            } else if *s == CliThreadState::Pausing {
                *s = CliThreadState::Paused;
                cvar.notify_all();
                log::trace!("confirmed pause");
            }

            while *s == CliThreadState::Paused {
                s = cvar.wait(s).unwrap();
            }
        }

        enable_raw_mode().unwrap();
        log::trace!("Raw mode enabled");
    }
}
