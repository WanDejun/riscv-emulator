use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode},
};

use crate::device::{DeviceTrait, Mem, uart::Uart16550};

pub struct Cli {
    pub uart: Uart16550,
}

impl Cli {
    pub fn new(rx_wiring: *const u8) -> Self {
        enable_raw_mode().unwrap();
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

impl Drop for Cli {
    fn drop(&mut self) {
        disable_raw_mode().unwrap(); // 恢复终端原始状态
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
        let mut cli = Cli::new(uart.get_tx_wiring());
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
        let mut cli = Cli::new((&rx) as *const u8);
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
