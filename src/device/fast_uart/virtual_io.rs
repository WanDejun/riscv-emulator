use crate::{
    device::{fast_uart::UartIOChannel, plic::ExternalInterrupt},
    device_poller::PollingEventTrait,
};

#[cfg_attr(feature = "native-cli", derive(clap::ValueEnum))]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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
        terminal_impl::poll_terminal(&mut self.channel)
    }
}

#[cfg(feature = "native-cli")]
mod terminal_impl {
    use crate::cli_coordinator::CliCoordinator;
    use crossterm::event::{self, Event, KeyCode};
    use std::{
        io::{self, Write},
        time::Duration,
    };

    use super::{ExternalInterrupt, UartIOChannel};

    pub(super) fn poll_terminal(channel: &mut UartIOChannel) -> Option<ExternalInterrupt> {
        CliCoordinator::global().confirm_pause_and_wait();

        loop {
            if !channel.busy.swap(true, std::sync::atomic::Ordering::AcqRel) {
                break;
            }
        }

        while let Ok(v) = channel.output_rx.try_recv() {
            print!("{}", v as char);
        }
        io::stdout().flush().unwrap();

        channel
            .busy
            .store(false, std::sync::atomic::Ordering::Release);

        let mut has_input = false;
        if event::poll(Duration::from_millis(20)).unwrap() {
            if let Event::Key(k) = event::read().unwrap() {
                has_input = true;
                match k.code {
                    KeyCode::Char(c) => channel.input_tx.send(c as u8).unwrap(),
                    KeyCode::Esc => channel.input_tx.send(0x1B).unwrap(),
                    KeyCode::Tab => channel.input_tx.send(b'\t').unwrap(),
                    KeyCode::Backspace => channel.input_tx.send(0x08).unwrap(),
                    KeyCode::Enter => channel.input_tx.send(b'\r').unwrap(),
                    KeyCode::Up => {
                        for v in [0x1B, 0x5B, 0x41] {
                            channel.input_tx.send(v).unwrap();
                        }
                    }
                    KeyCode::Down => {
                        for v in [0x1B, 0x5B, 0x42] {
                            channel.input_tx.send(v).unwrap();
                        }
                    }
                    KeyCode::Left => {
                        for v in [0x1B, 0x5B, 0x44] {
                            channel.input_tx.send(v).unwrap();
                        }
                    }
                    KeyCode::Right => {
                        for v in [0x1B, 0x5B, 0x43] {
                            channel.input_tx.send(v).unwrap();
                        }
                    }
                    _ => {
                        has_input = false;
                    }
                }
            }
        }

        let ier = channel.ier.load(std::sync::atomic::Ordering::Acquire);
        let thre_interrupt = ier & 0x02 != 0
            && channel
                .thre_pending
                .load(std::sync::atomic::Ordering::Acquire);
        let rda_interrupt = ier & 0x01 != 0 && has_input;

        if thre_interrupt || rda_interrupt {
            log::trace!(
                "[UART-poll] firing IRQ {}: thre={} rda={} ier={:#04x}",
                channel.interrupt_id,
                thre_interrupt,
                rda_interrupt,
                ier
            );
            Some(channel.interrupt_id)
        } else {
            None
        }
    }
}

#[cfg(not(feature = "native-cli"))]
mod terminal_impl {
    use super::{ExternalInterrupt, UartIOChannel};

    pub(super) fn poll_terminal(_channel: &mut UartIOChannel) -> Option<ExternalInterrupt> {
        None
    }
}
