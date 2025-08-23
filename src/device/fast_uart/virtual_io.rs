use crate::cli_coordinator::CliCoordinator;
use clap::ValueEnum;
use crossbeam::channel::{Receiver, Sender};
use crossterm::event::{self, Event, KeyCode};
use lazy_static::lazy_static;
use std::{
    io::{self, Write},
    sync::{Arc, Mutex, atomic::AtomicBool},
    thread,
    time::Duration,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum SerialDestination {
    Test,
    Stdio,
}

pub(super) fn spawn_io_thread(
    input_tx: Sender<u8>,
    output_rx: Receiver<u8>,
    sync_lock: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        loop {
            CliCoordinator::global().confirm_pause_and_wait();

            // output epoll
            loop {
                // lock
                if !sync_lock.swap(true, std::sync::atomic::Ordering::AcqRel) {
                    break;
                }
            }
            while let Ok(v) = output_rx.try_recv() {
                print!("{}", v as char);
            }
            io::stdout().flush().unwrap();
            sync_lock.store(false, std::sync::atomic::Ordering::Release);

            // input epoll
            if event::poll(Duration::from_millis(20)).unwrap() {
                if let Event::Key(k) = event::read().unwrap() {
                    match k.code {
                        KeyCode::Char(c) => input_tx.send(c as u8).unwrap(),
                        KeyCode::Tab => input_tx.send(b'\t').unwrap(),
                        KeyCode::Backspace => input_tx.send(0x08).unwrap(),
                        KeyCode::Enter => input_tx.send(b'\r').unwrap(),
                        KeyCode::Up => {
                            for v in [0x1B, 0x5B, 0x41] {
                                input_tx.send(v).unwrap();
                            }
                        }
                        KeyCode::Down => {
                            for v in [0x1B, 0x5B, 0x42] {
                                input_tx.send(v).unwrap();
                            }
                        }
                        KeyCode::Left => {
                            for v in [0x1B, 0x5B, 0x43] {
                                input_tx.send(v).unwrap();
                            }
                        }
                        KeyCode::Right => {
                            for v in [0x1B, 0x5B, 0x44] {
                                input_tx.send(v).unwrap();
                            }
                        }
                        _ => {}
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    });
}

lazy_static! {
    pub static ref SIMULATION_IO: Mutex<SimulationIO> = Mutex::new(SimulationIO::new());
}

/// # SimulationIO
/// Mainly used in debugging.
pub struct SimulationIO {
    tx_rx: Option<(Sender<u8>, Receiver<u8>)>,
}

impl SimulationIO {
    fn new() -> Self {
        Self { tx_rx: None }
    }

    pub fn set(&mut self, tx_rx: Option<(Sender<u8>, Receiver<u8>)>) {
        assert!(
            crate::EMULATOR_CONFIG.lock().unwrap().serial_destination == SerialDestination::Test
        );
        self.tx_rx = tx_rx;
    }

    pub fn send_input_data<T>(&self, data: T)
    where
        T: IntoIterator<Item = u8>,
    {
        let input_tx = &self.tx_rx.as_ref().unwrap().0;
        for byte in data.into_iter() {
            input_tx.send(byte);
        }
    }

    pub fn receive_output_data(&self) -> Vec<u8> {
        let output_rx = &self.tx_rx.as_ref().unwrap().1;
        let mut datas = Vec::new();
        while let Ok(data) = output_rx.try_recv() {
            datas.push(data);
        }

        datas
    }
}
