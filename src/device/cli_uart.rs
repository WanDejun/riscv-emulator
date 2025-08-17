use std::{
    collections::VecDeque,
    io::{self, Write},
    thread,
    time::Duration,
};

use crossbeam::channel::{self, Receiver, Sender};
use crossterm::event::{self, Event, KeyCode};
#[cfg(not(test))]
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

use crate::{
    device::{DeviceTrait, Mem, uart::Uart16550},
    handle_trait::HandleTrait,
    utils::read_bit,
};

pub struct CliUart {
    pub uart: Uart16550,
    pub(super) input_tx: Sender<u8>,
    input_rx: Receiver<u8>,
    output_tx: Sender<u8>,
    pub(super) output_rx: Receiver<u8>,
}

impl CliUart {
    pub fn new(rx_wiring: *const u8) -> Self {
        let (input_tx, input_rx) = channel::unbounded();
        let (output_tx, output_rx) = channel::unbounded();
        Self {
            uart: Uart16550::new(rx_wiring),
            input_tx,
            input_rx,
            output_tx,
            output_rx,
        }
    }
    pub fn step(&mut self) {
        self.uart.step();

        // Output
        if (self.uart.read::<u8>(5).unwrap() & 0b1) != 0 {
            self.output_tx
                .send(self.uart.read::<u8>(0).unwrap())
                .unwrap();
        }

        // Input
        if self.uart.transmit_holding_empty() {
            if let Ok(v) = self.input_rx.try_recv() {
                self.uart.write(0x00, v).unwrap();
            }
        }
    }

    pub fn sync(&mut self) {
        while !read_bit(&self.uart.read::<u8>(5).unwrap(), 6) || !self.output_rx.is_empty() {
            self.step();
        }
    }
}

pub fn spawn_io_thread(input_tx: Sender<u8>, output_rx: Receiver<u8>) {
    thread::spawn(move || {
        loop {
            // output epoll
            while let Ok(v) = output_rx.try_recv() {
                print!("{}", v as char);
            }
            io::stdout().flush().unwrap();

            // input epoll
            if event::poll(Duration::from_millis(100)).unwrap() {
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
            // std::thread::sleep(Duration::from_millis(1));
        }
    });
}

/// Set terminal to raw mode. RAII to unset terminal raw mode.
pub struct CliUartHandle {}
impl CliUartHandle {
    pub fn new() -> Self {
        #[cfg(not(test))]
        enable_raw_mode().unwrap();

        Self {}
    }
}
impl HandleTrait for CliUartHandle {}
impl Drop for CliUartHandle {
    fn drop(&mut self) {
        #[cfg(not(test))]
        disable_raw_mode().unwrap(); // 恢复终端原始状态
    }
}

/// # FIFOUart
/// - for test, easy to set input and get output from raw test code.
/// DO NOT input/output to terminal. BUT to inner fifo.
/// ```no_run
/// # use riscv_emulator::device::cli_uart::FIFOUart;
/// # let rx_wiring: *const u8 = std::ptr::null();
/// let mut debug_uart = FIFOUart::new(rx_wiring);
/// // Equal type into terminal
/// debug_uart.send('a' as u8);
/// // Equal get output from terminal
/// // BUT `debug_uart.receive` is non-blocking function
/// debug_uart.receive();
/// ```
pub struct FIFOUart {
    pub uart: Uart16550,
    input_fifo: VecDeque<u8>,
    output_fifo: VecDeque<u8>,
}

impl FIFOUart {
    pub fn new(rx_wiring: *const u8) -> Self {
        Self {
            uart: Uart16550::new(rx_wiring),
            input_fifo: VecDeque::new(),
            output_fifo: VecDeque::new(),
        }
    }
    pub fn step(&mut self) {
        self.uart.step();

        // Output
        if (self.uart.read::<u8>(5).unwrap() & 0b1) != 0 {
            self.output_fifo.push_back(self.uart.read::<u8>(0).unwrap());
        }

        // Input
        if (self.uart.read::<u8>(5).unwrap() & 0x40) != 0 {
            if let Some(val) = self.input_fifo.pop_front() {
                self.uart.write(0x00, val).unwrap();
            }
        }
    }

    pub fn sync(&mut self) {
        while !read_bit(&self.uart.read::<u8>(5).unwrap(), 6) || !self.output_fifo.is_empty() {
            self.step();
        }
    }

    pub fn send(&mut self, data: u8) {
        self.input_fifo.push_back(data);
    }

    pub fn receive(&mut self) -> Option<u8> {
        self.output_fifo.pop_front()
    }
}

#[cfg(test)]
mod test {
    use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

    use crate::device::config::UART_DEFAULT_DIV;

    use super::*;

    #[ignore = "debug"]
    #[test]
    /// just for debug, not an test.
    fn cli_output_test() {
        enable_raw_mode().unwrap();
        let rx = 1u8;
        let mut uart = Uart16550::new((&rx) as *const u8);
        let mut cli = CliUart::new(uart.get_tx_wiring());
        uart.write(0, 'a' as u8).unwrap();
        for _ in 0..20 {
            for _ in 0..UART_DEFAULT_DIV * 16 {
                cli.step();
                uart.step();
            }
        }
        disable_raw_mode().unwrap();
    }

    #[ignore = "debug"]
    #[test]
    /// just for debug, not an test.
    fn cli_input_test() {
        enable_raw_mode().unwrap();
        let rx = 1u8;
        let mut cli = CliUart::new((&rx) as *const u8);
        let mut uart = Uart16550::new(cli.uart.get_tx_wiring());
        let _tx_wriing = cli.uart.get_tx_wiring();
        loop {
            cli.step();
            uart.step();
            if (uart.read::<u8>(5).unwrap() & 0x01) != 0 {
                let v = uart.read::<u8>(0).unwrap();
                println!("{}", v);
                break;
            }
        }
        disable_raw_mode().unwrap();
    }
}
