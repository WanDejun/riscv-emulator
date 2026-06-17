use super::*;

use crossbeam::channel::{self, Receiver, Sender};
use std::collections::VecDeque;

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
