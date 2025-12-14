use crate::{
    cli_coordinator::CliCoordinator,
    device::{fast_uart::UartIOChannel, plic::ExternalInterrupt},
    device_poller::PollingEventTrait,
};
use clap::ValueEnum;
use crossterm::event::{self, Event, KeyCode};
use std::{
    io::{self, Write},
    time::Duration,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum SerialDestination {
    Test,
    Stdio,
}

pub struct TerminalIO {
    channel: UartIOChannel,
}

impl TerminalIO {
    pub fn new(channel: UartIOChannel) -> Self {
        Self { channel }
    }
}

/// # SimulationIO
/// Mainly used in debugging.
pub struct SimulationIO {
    channel: UartIOChannel,
}

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
            let _ = input_tx.send(byte);
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

impl PollingEventTrait for TerminalIO {
    fn poll(&mut self) -> Option<ExternalInterrupt> {
        // TODO: CliCoordinator and TerminalIO::UartIOChannel are both coordinating the usage of terminal.
        // We should merge them in the future.
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
        None
    }
}
