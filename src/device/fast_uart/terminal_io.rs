use std::collections::VecDeque;

use crossbeam::channel::{self, Receiver, Sender};

pub struct ReceiveGuard<'a, S: ByteSink + ?Sized> {
    sink: &'a mut S,
    has_received: bool,
}

impl<'a, S: ByteSink + ?Sized> ReceiveGuard<'a, S> {
    fn new(sink: &'a mut S) -> Self {
        sink.before_receive();
        Self {
            sink,
            has_received: false,
        }
    }

    pub fn receive(&mut self, byte: u8) {
        self.sink.do_receive(byte);
        self.has_received = true;
    }
}

impl<'a, S: ByteSink + ?Sized> Drop for ReceiveGuard<'a, S> {
    fn drop(&mut self) {
        self.sink.after_receive(self.has_received);
    }
}

/// Use [`ByteSinkExt::receive_guard`] to automatically call [`Self::before_receive`] and [`Self::after_receive`] with RAII.
/// [`Self::before_receive`] and [`Self::after_receive`] are only guarenteed appear in pair.
/// They are allowed to be called when nothing received, based on the implementation of [`ByteSource`].
pub trait ByteSink {
    /// DO NOT USE this method directly, prefer [`ByteSinkExt::receive_guard`].
    fn do_receive(&mut self, byte: u8);
    fn before_receive(&mut self);
    fn after_receive(&mut self, has_received: bool);
}

pub trait ByteSinkExt: ByteSink {
    #[inline]
    #[must_use]
    fn receive_guard(&mut self) -> ReceiveGuard<'_, Self> {
        ReceiveGuard::new(self)
    }

    fn receive_bytes(&mut self, bytes: impl IntoIterator<Item = u8>) {
        let mut guard = self.receive_guard();
        for byte in bytes.into_iter() {
            guard.receive(byte);
        }
    }
}

impl<S: ByteSink + ?Sized> ByteSinkExt for S {}

impl ByteSink for VecDeque<u8> {
    #[inline]
    fn do_receive(&mut self, byte: u8) {
        self.push_back(byte);
    }

    fn before_receive(&mut self) {}
    fn after_receive(&mut self, _received: bool) {}
}

impl ByteSink for Vec<u8> {
    #[inline]
    fn do_receive(&mut self, byte: u8) {
        self.push(byte);
    }

    fn before_receive(&mut self) {}
    fn after_receive(&mut self, _received: bool) {}
}

pub trait ByteSource {
    fn drain_to(&mut self, target: &mut dyn ByteSink) -> bool;
}

#[derive(Clone)]
pub struct ChannelIOContext {
    pub output_sender: Sender<u8>,
    pub input_receiver: Receiver<u8>,
}

impl ChannelIOContext {
    pub fn new() -> (ChannelIOContext, ChannelIOContext) {
        let (output_tx, output_rx) = channel::unbounded();
        let (input_tx, input_rx) = channel::unbounded();

        return (
            Self {
                output_sender: output_tx,
                input_receiver: input_rx,
            },
            Self {
                output_sender: input_tx,
                input_receiver: output_rx,
            },
        );
    }
}

impl ByteSink for ChannelIOContext {
    #[inline]
    fn do_receive(&mut self, byte: u8) {
        let _ = self.output_sender.send(byte);
    }

    fn before_receive(&mut self) {}
    fn after_receive(&mut self, _received: bool) {}
}

impl ByteSource for ChannelIOContext {
    fn drain_to(&mut self, target: &mut dyn ByteSink) -> bool {
        let mut guard = target.receive_guard();
        while let Ok(byte) = self.input_receiver.try_recv() {
            guard.receive(byte);
        }

        guard.has_received
    }
}

#[cfg(feature = "native-cli")]
pub mod native {
    use super::*;
    use crate::cli_coordinator::CliCoordinator;
    use crossterm::event::{self, Event, KeyCode};
    use std::{io::Write, time::Duration};

    pub struct TerminalIOContext;

    impl ByteSink for TerminalIOContext {
        #[inline]
        fn before_receive(&mut self) {
            CliCoordinator::global().confirm_pause_and_wait();
        }

        #[inline]
        fn do_receive(&mut self, byte: u8) {
            log::trace!("[TerminalIO] char {:?} received", byte as char);
            print!("{}", byte as char);
        }

        #[inline]
        fn after_receive(&mut self, _received: bool) {
            log::trace!("[TerminalIO] flushing");
            std::io::stdout().flush().unwrap();
        }
    }

    impl ByteSource for TerminalIOContext {
        #[inline]
        fn drain_to(&mut self, target: &mut dyn ByteSink) -> bool {
            let mut guard = target.receive_guard();

            if !event::poll(Duration::from_millis(0)).unwrap_or(false) {
                return false;
            }

            let Ok(Event::Key(k)) = event::read() else {
                return false;
            };

            match k.code {
                KeyCode::Char(c) => guard.receive(c as u8),
                KeyCode::Esc => guard.receive(0x1B),
                KeyCode::Tab => guard.receive(b'\t'),
                KeyCode::Backspace => guard.receive(0x08),
                KeyCode::Enter => guard.receive(b'\r'),
                KeyCode::Up => {
                    for v in [0x1B, 0x5B, 0x41] {
                        guard.receive(v);
                    }
                }
                KeyCode::Down => {
                    for v in [0x1B, 0x5B, 0x42] {
                        guard.receive(v);
                    }
                }
                KeyCode::Left => {
                    for v in [0x1B, 0x5B, 0x44] {
                        guard.receive(v);
                    }
                }
                KeyCode::Right => {
                    for v in [0x1B, 0x5B, 0x43] {
                        guard.receive(v);
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

#[cfg(test)]
mod test {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    struct MockByteSink {
        before_called: bool,
        received: Vec<u8>,
        after_called: bool,
        has_received: bool,
    }

    impl MockByteSink {
        fn new() -> Self {
            Self {
                before_called: false,
                received: vec![],
                after_called: false,
                has_received: false,
            }
        }
    }

    impl ByteSink for MockByteSink {
        fn before_receive(&mut self) {
            self.before_called = true;
        }

        fn do_receive(&mut self, byte: u8) {
            self.received.push(byte);
        }

        fn after_receive(&mut self, has_received: bool) {
            self.after_called = true;
            self.has_received = has_received;
        }
    }

    struct MockByteSource {
        bytes: Vec<u8>,
    }

    impl ByteSource for MockByteSource {
        fn drain_to(&mut self, target: &mut dyn ByteSink) -> bool {
            let mut guard = target.receive_guard();
            for byte in self.bytes.iter() {
                guard.receive(*byte);
            }
            guard.has_received
        }
    }

    #[test]
    fn test_byte_source() {
        let mut src = MockByteSource {
            bytes: vec![1, 2, 3],
        };
        let mut sink = MockByteSink::new();
        let sink_ref: &mut dyn ByteSink = &mut sink;

        assert!(src.drain_to(sink_ref));
        assert_eq!(
            sink,
            MockByteSink {
                before_called: true,
                after_called: true,
                received: vec![1, 2, 3],
                has_received: true,
            }
        );
    }
}
