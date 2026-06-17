mod common;

#[cfg(feature = "native-cli")]
mod terminal_io;

pub use common::*;
pub use terminal_io::*;

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

    #[inline]
    pub fn receive(&mut self, byte: u8) {
        self.sink.do_receive(byte);
        self.has_received = true;
    }

    #[inline]
    pub fn receives(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.receive(byte);
        }
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

pub trait ByteSource {
    fn drain_to(&mut self, target: &mut dyn ByteSink) -> bool;
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
