//! TODO:
//! This implementation use `crossterm` to parse ANSI sequence to Rust struct, then parse it back.
//! After implementing [`super::StdinRouter`], they are dead code now.
//! maybe we can use this as a fallback for old version Windows that doesn't support ANSI.

use super::*;
use crate::device::power_manager::{POWER_OFF_CODE, POWER_STATUS};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::sync::atomic::Ordering;
use std::{io::Write, time::Duration};

/// Host-side terminal interface bridging stdin/stdout with the UART.
///
/// Implements a QEMU-like escape sequence: `Ctrl+A` prefixes a one-key command.
/// `x` quits the emulator; any other key is forwarded to the guest as-is.
pub struct TerminalIOContext {
    /// True after `Ctrl+A` has been seen, awaiting the command key.
    escape_pending: bool,
}

impl TerminalIOContext {
    pub fn new() -> Self {
        Self {
            escape_pending: false,
        }
    }
}

impl ByteSink for TerminalIOContext {
    #[inline]
    fn before_receive(&mut self) {}

    #[inline]
    fn do_receive(&mut self, bytes: &[u8]) {
        // do not use `print!` because we need to output the raw byte sequence.
        std::io::stdout().write_all(bytes).unwrap();
    }

    #[inline]
    fn after_receive(&mut self, _received: bool) {
        std::io::stdout().flush().unwrap();
    }
}

/// Map a character to the control byte a terminal emits for `Ctrl+<key>`
fn ctrl_byte(c: char) -> u8 {
    match c {
        '@'..='_' | 'a'..='z' => (c as u8) & 0x1F,
        ' ' => 0x00,
        '?' => 0x7F,
        _ => {
            log::warn!("unrecognized control code character: {c}");
            c as u8
        }
    }
}

fn calc_modifiers(modifiers: KeyModifiers) -> Option<u8> {
    let mut value = 1;
    if modifiers.contains(KeyModifiers::SHIFT) {
        value += 1;
    }
    if modifiers.contains(KeyModifiers::ALT) {
        value += 2;
    }
    if modifiers.contains(KeyModifiers::CONTROL) {
        value += 4;
    }
    (value != 1).then_some(value)
}

fn receive_key(k: KeyEvent, target: &mut dyn ByteSink) -> bool {
    let mut guard = target.receive_guard();

    // control sequence introducer (CSI)
    // ignore first when no modifier
    let emit_csi_1 = |first: u8, name: u8, mut guard: ReceiveGuard<_>| {
        guard.receive(&[0x1B, b'[']);
        if let Some(value) = calc_modifiers(k.modifiers) {
            guard.receive(&[first, b';', value + b'0']);
        }
        guard.receive_byte(name);
    };

    // don't ignore first when no modifier
    let emit_csi_2 = |first: u8, name: u8, mut guard: ReceiveGuard<_>| {
        guard.receive(&[0x1B, b'[', first]);
        if let Some(value) = calc_modifiers(k.modifiers) {
            guard.receive(&[b';', value + b'0']);
        }
        guard.receive_byte(name);
    };

    match k.code {
        KeyCode::Char(c) => {
            if k.modifiers.contains(KeyModifiers::ALT) {
                guard.receive_byte(0x1B);
            }
            if k.modifiers.contains(KeyModifiers::CONTROL) {
                guard.receive_byte(ctrl_byte(c));
            } else {
                let mut buf = [0; 4];
                for &byte in c.encode_utf8(&mut buf).as_bytes() {
                    guard.receive_byte(byte);
                }
            }
        }

        KeyCode::Tab => guard.receive_byte(b'\t'),
        KeyCode::Enter => {
            if k.modifiers.contains(KeyModifiers::ALT) {
                guard.receive_byte(0x1B);
            }

            if k.modifiers.contains(KeyModifiers::CONTROL) {
                guard.receive_byte(b'\n')
            } else {
                guard.receive_byte(b'\r')
            }
        }

        KeyCode::Backspace => guard.receive_byte(0x7f),
        KeyCode::Pause => guard.receive_byte(0x1a),
        KeyCode::Esc => guard.receive_byte(0x1b),

        KeyCode::Insert => emit_csi_2(b'2', b'~', guard),
        KeyCode::Delete => emit_csi_2(b'3', b'~', guard),
        KeyCode::PageUp => emit_csi_2(b'5', b'~', guard),
        KeyCode::PageDown => emit_csi_2(b'6', b'~', guard),

        KeyCode::Up => emit_csi_1(b'1', b'A', guard),
        KeyCode::Down => emit_csi_1(b'1', b'B', guard),
        KeyCode::Right => emit_csi_1(b'1', b'C', guard),
        KeyCode::Left => emit_csi_1(b'1', b'D', guard),
        KeyCode::Home => emit_csi_1(b'1', b'H', guard),
        KeyCode::End => emit_csi_1(b'1', b'F', guard),

        _ => {
            log::warn!("unrecognized key code: {:?}", k.code);
            return false;
        }
    }

    true
}

impl ByteSource for TerminalIOContext {
    #[inline]
    fn drain_to<S: ByteSink>(&mut self, target: &mut S) -> bool {
        if !event::poll(Duration::from_millis(0)).unwrap_or(false) {
            return false;
        }

        let Ok(Event::Key(k)) = event::read() else {
            return false;
        };

        // QEMU-style escape: Ctrl+A prefixes a one-key command.
        if self.escape_pending {
            self.escape_pending = false;
            match k.code {
                KeyCode::Char('x') => {
                    log::info!("[TerminalIO] Ctrl+A x — requesting exit");
                    POWER_STATUS.store(POWER_OFF_CODE, Ordering::Release);
                    return false;
                }
                _ => {
                    // other key: do nothing to forward the key to the guest as-is.
                }
            }
        } else if k.modifiers == KeyModifiers::CONTROL && k.code == KeyCode::Char('a') {
            log::info!("[TerminalIO] Ctrl+A — awaiting command key");
            self.escape_pending = true;
            return false;
        }

        receive_key(k, target)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn encode(key_code: KeyCode, modifiers: KeyModifiers) -> Vec<u8> {
        let mut bytes = Vec::new();
        let _ = receive_key(KeyEvent::new(key_code, modifiers), &mut bytes);
        bytes
    }

    fn test_key(key_code: KeyCode, modifiers: KeyModifiers, seq: &str) {
        let bytes = encode(key_code, modifiers);
        assert_eq!(
            bytes.as_slice(),
            seq.as_bytes(),
            "key = {key_code:?}, modifiers = {modifiers:?}"
        );
    }

    #[test]
    fn test_key_encode() {
        #[allow(non_snake_case)]
        let CONTROL_ALT = KeyModifiers::CONTROL | KeyModifiers::ALT;

        // common
        test_key(KeyCode::Char('c'), KeyModifiers::NONE, "c");
        test_key(KeyCode::Char('c'), KeyModifiers::CONTROL, "\x03");
        test_key(KeyCode::Char('c'), CONTROL_ALT, "\x1b\x03");

        // arrows
        test_key(KeyCode::Left, KeyModifiers::NONE, "\x1b[D");
        test_key(KeyCode::Left, KeyModifiers::CONTROL, "\x1b[1;5D");
        test_key(KeyCode::Left, CONTROL_ALT, "\x1b[1;7D");

        // special
        test_key(KeyCode::Delete, KeyModifiers::NONE, "\x1b[3~");
        test_key(KeyCode::Delete, KeyModifiers::CONTROL, "\x1b[3;5~");

        test_key(KeyCode::Backspace, KeyModifiers::NONE, "\x7f");
    }
}
