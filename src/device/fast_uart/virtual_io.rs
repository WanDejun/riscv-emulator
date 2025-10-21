use crate::{
    async_poller::{InterryptID, PollingEventTrait},
    cli_coordinator::CliCoordinator,
    device::fast_uart::UartIOChannel,
};
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

pub trait SerialDestTrait {}

pub struct TerminalIO {
    channel: UartIOChannel,
}

impl SerialDestTrait for TerminalIO {}
impl TerminalIO {
    pub fn new(channel: UartIOChannel) -> Self {
        Self { channel }
    }
}

fn spawn_io_thread(input_tx: Sender<u8>, output_rx: Receiver<u8>, sync_lock: Arc<AtomicBool>) {
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

/// # SimulationIO
/// Mainly used in debugging.
pub struct SimulationIO {
    channel: UartIOChannel,
}

impl SerialDestTrait for SimulationIO {}

impl SimulationIO {
    pub fn new(channel: UartIOChannel) -> Self {
        Self { channel }
    }

    pub fn send_input_data<T>(&self, data: T)
    where
        T: IntoIterator<Item = u8>,
    {
        let input_tx = &self.channel.input_tx;
        for byte in data.into_iter() {
            input_tx.send(byte);
        }
    }

    pub fn receive_output_data(&self) -> Vec<u8> {
        let output_rx = &self.channel.output_rx;
        let mut datas = Vec::new();
        while let Ok(data) = output_rx.try_recv() {
            datas.push(data);
        }

        datas
    }
}

// pub struct UartPollingEvent {
//     // pub poll_function: fn(Sender<u8>, Receiver<u8>, Arc<AtomicBool>),
//     pub input_tx: Sender<u8>,
//     pub output_rx: Receiver<u8>,
//     pub sync_lock: Arc<AtomicBool>,
// }

impl PollingEventTrait for TerminalIO {
    fn poll(&self) -> Option<InterryptID> {
        CliCoordinator::global().confirm_pause_and_wait();

        // output epoll
        loop {
            // lock
            if !self
                .channel
                .busy
                .swap(true, std::sync::atomic::Ordering::AcqRel)
            {
                break;
            }
        }
        while let Ok(v) = self.channel.output_rx.try_recv() {
            print!("{}", v as char);
        }
        io::stdout().flush().unwrap();
        self.channel
            .busy
            .store(false, std::sync::atomic::Ordering::Release);

        // input epoll
        if event::poll(Duration::from_millis(20)).unwrap() {
            if let Event::Key(k) = event::read().unwrap() {
                match k.code {
                    KeyCode::Char(c) => self.channel.input_tx.send(c as u8).unwrap(),
                    KeyCode::Tab => self.channel.input_tx.send(b'\t').unwrap(),
                    KeyCode::Backspace => self.channel.input_tx.send(0x08).unwrap(),
                    KeyCode::Enter => self.channel.input_tx.send(b'\r').unwrap(),
                    KeyCode::Up => {
                        for v in [0x1B, 0x5B, 0x41] {
                            self.channel.input_tx.send(v).unwrap();
                        }
                    }
                    KeyCode::Down => {
                        for v in [0x1B, 0x5B, 0x42] {
                            self.channel.input_tx.send(v).unwrap();
                        }
                    }
                    KeyCode::Left => {
                        for v in [0x1B, 0x5B, 0x43] {
                            self.channel.input_tx.send(v).unwrap();
                        }
                    }
                    KeyCode::Right => {
                        for v in [0x1B, 0x5B, 0x44] {
                            self.channel.input_tx.send(v).unwrap();
                        }
                    }
                    _ => {}
                }
            }
        }
        std::thread::sleep(Duration::from_millis(2));
        None
    }
}
