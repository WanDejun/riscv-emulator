use std::{collections::VecDeque, time::Duration};

use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode},
    tty::IsTty,
};

use crate::{
    device::{DeviceTrait, Mem, uart::Uart16550},
    handle_trait::HandleTrait,
};

pub struct CliUart {
    pub uart: Uart16550,
}

impl CliUart {
    pub fn new(rx_wiring: *const u8) -> Self {
        Self {
            uart: Uart16550::new(rx_wiring),
        }
    }
    pub fn one_shot(&mut self) {
        self.uart.one_shot();

        // Output
        if (self.uart.read::<u8>(5) & 0b1) != 0 {
            print!("{}", self.uart.read::<u8>(0) as char);
        }

        // Input
        if event::poll(Duration::from_millis(0)).unwrap() {
            if let Event::Key(k) = event::read().unwrap() {
                if let KeyCode::Char(c) = k.code {
                    self.uart.write(0x00, c as u8);
                }
            }
        }
    }
}

/// Set terminal to raw mode. RAII to unset terminal raw mode.
pub struct CliUartHandle {}
impl CliUartHandle {
    pub fn new() -> Self {
        if std::io::stdin().is_tty() {
            enable_raw_mode().unwrap();
        }
        Self {}
    }
}
impl HandleTrait for CliUartHandle {}
impl Drop for CliUartHandle {
    fn drop(&mut self) {
        if std::io::stdin().is_tty() {
            disable_raw_mode().unwrap(); // 恢复终端原始状态
        }
    }
}

/// # FIFOUart
/// - for test, easy to set input and get output from raw test code.
/// DO NOT input/output to terminal. BUT to inner fifo.
/// ```
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
    pub fn one_shot(&mut self) {
        self.uart.one_shot();

        // Output
        if (self.uart.read::<u8>(5) & 0b1) != 0 {
            self.output_fifo.push_back(self.uart.read::<u8>(0));
        }

        // Input
        if (self.uart.read::<u8>(5) & 0x40) != 0 {
            if let Some(val) = self.input_fifo.pop_front() {
                self.uart.write(0x00, val);
            }
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
    use crate::device::config::UART_DEFAULT_DIV;

    use super::*;

    #[ignore = "debug"]
    #[test]
    /// just for debug, not an test.
    fn cli_output_test() {
        let rx = 1u8;
        let mut uart = Uart16550::new((&rx) as *const u8);
        let mut cli = CliUart::new(uart.get_tx_wiring());
        uart.write(0, 'a' as u8);
        for _ in 0..20 {
            for _ in 0..UART_DEFAULT_DIV * 16 {
                cli.one_shot();
                uart.one_shot();
            }
        }
    }

    #[ignore = "debug"]
    #[test]
    /// just for debug, not an test.
    fn cli_input_test() {
        let rx = 1u8;
        let mut cli = CliUart::new((&rx) as *const u8);
        let mut uart = Uart16550::new(cli.uart.get_tx_wiring());
        let _tx_wriing = cli.uart.get_tx_wiring();
        loop {
            cli.one_shot();
            uart.one_shot();
            if (uart.read::<u8>(5) & 0x01) != 0 {
                let v = uart.read::<u8>(0);
                println!("{}", v);
                break;
            }
        }
    }
}
