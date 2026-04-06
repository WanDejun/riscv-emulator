use crate::device::fast_uart::UartIOChannel;
use crossbeam::channel::{self, Receiver, Sender};

pub(crate) struct TerminalPollResult {
    pub has_input: bool,
}

pub(crate) struct TerminalIO<IO: HostSerialIO = DefaultHostSerialIO> {
    channel: UartIOChannel,
    host_io: IO,
}

impl<IO: HostSerialIO> TerminalIO<IO> {
    pub fn new(channel: UartIOChannel, host_io: IO) -> Self {
        Self { channel, host_io }
    }

    pub fn poll_nonblocking(&mut self) -> TerminalPollResult {
        imp::before_poll();

        loop {
            if !self
                .channel
                .busy
                .swap(true, std::sync::atomic::Ordering::AcqRel)
            {
                break;
            }
        }

        self.host_io.flush_output_nonblocking(&mut self.channel);

        self.channel
            .busy
            .store(false, std::sync::atomic::Ordering::Release);

        let has_input = self.host_io.poll_input_nonblocking(&mut self.channel);
        TerminalPollResult { has_input }
    }
}

/// Host-side serial interaction strategy.
///
/// All methods must be non-blocking so they are safe in single-threaded stepping.
pub(crate) trait HostSerialIO: Send {
    /// Drain pending UART output bytes to host side.
    fn flush_output_nonblocking(&mut self, channel: &mut UartIOChannel);

    /// Try to fetch host input and enqueue it to UART RX path.
    /// Returns true if input was received in this call.
    fn poll_input_nonblocking(&mut self, channel: &mut UartIOChannel) -> bool;
}

#[derive(Clone)]
pub(crate) struct BufferedSerialHandle {
    input_tx: Sender<u8>,
    output_rx: Receiver<u8>,
}

impl BufferedSerialHandle {
    pub fn send_input_data<T>(&self, data: T)
    where
        T: IntoIterator<Item = u8>,
    {
        for byte in data {
            let _ = self.input_tx.send(byte);
        }
    }

    pub fn receive_output_data(&self) -> Vec<u8> {
        let mut datas = Vec::new();
        while let Ok(data) = self.output_rx.try_recv() {
            datas.push(data);
        }
        datas
    }
}

pub(crate) struct BufferedHostSerialIO {
    input_rx: Receiver<u8>,
    output_tx: Sender<u8>,
}

impl BufferedHostSerialIO {
    fn new_with_handle() -> (Self, BufferedSerialHandle) {
        let (input_tx, input_rx) = channel::unbounded();
        let (output_tx, output_rx) = channel::unbounded();
        (
            Self {
                input_rx,
                output_tx,
            },
            BufferedSerialHandle {
                input_tx,
                output_rx,
            },
        )
    }
}

impl HostSerialIO for BufferedHostSerialIO {
    fn flush_output_nonblocking(&mut self, channel: &mut UartIOChannel) {
        while let Ok(v) = channel.output_rx.try_recv() {
            let _ = self.output_tx.send(v);
        }
    }

    fn poll_input_nonblocking(&mut self, channel: &mut UartIOChannel) -> bool {
        let mut has_input = false;
        while let Ok(v) = self.input_rx.try_recv() {
            has_input = true;
            let _ = channel.input_tx.send(v);
        }
        has_input
    }
}

pub(crate) use imp::{DefaultHostSerialIO, create_default_host_serial_io};

#[cfg(all(feature = "native-cli", not(test)))]
mod imp {
    use crate::cli_coordinator::CliCoordinator;
    use crossterm::event::{self, Event, KeyCode};
    use std::{
        io::{self, Write},
        time::Duration,
    };

    use super::{BufferedSerialHandle, HostSerialIO, UartIOChannel};

    pub(crate) struct NativeTerminalIo;

    pub(crate) type DefaultHostSerialIO = NativeTerminalIo;

    pub(crate) fn create_default_host_serial_io()
    -> (DefaultHostSerialIO, Option<BufferedSerialHandle>) {
        (NativeTerminalIo, None)
    }

    pub(super) fn before_poll() {
        CliCoordinator::global().confirm_pause_and_wait();
    }

    impl HostSerialIO for NativeTerminalIo {
        fn flush_output_nonblocking(&mut self, channel: &mut UartIOChannel) {
            while let Ok(v) = channel.output_rx.try_recv() {
                print!("{}", v as char);
            }
            io::stdout().flush().unwrap();
        }

        fn poll_input_nonblocking(&mut self, channel: &mut UartIOChannel) -> bool {
            if !event::poll(Duration::from_millis(0)).unwrap() {
                return false;
            }

            let Event::Key(k) = event::read().unwrap() else {
                return false;
            };

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
                    return false;
                }
            }

            true
        }
    }
}

#[cfg(any(not(feature = "native-cli"), test))]
mod imp {
    use super::{BufferedHostSerialIO, BufferedSerialHandle};

    pub(crate) type DefaultHostSerialIO = BufferedHostSerialIO;

    pub(crate) fn create_default_host_serial_io()
    -> (DefaultHostSerialIO, Option<BufferedSerialHandle>) {
        let (host_io, handle) = BufferedHostSerialIO::new_with_handle();
        (host_io, Some(handle))
    }

    pub(super) fn before_poll() {}
}
