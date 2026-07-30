use super::*;

use std::collections::VecDeque;

impl ByteSink for VecDeque<u8> {
    #[inline]
    fn do_receive(&mut self, bytes: &[u8]) {
        self.extend(bytes);
    }

    fn before_receive(&mut self) {}
    fn after_receive(&mut self, _received: bool) {}
}

impl ByteSink for Vec<u8> {
    #[inline]
    fn do_receive(&mut self, bytes: &[u8]) {
        self.extend(bytes);
    }

    fn before_receive(&mut self) {}
    fn after_receive(&mut self, _received: bool) {}
}
