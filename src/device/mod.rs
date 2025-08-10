use std::sync::{Arc, Mutex};

use crossterm::{terminal::enable_raw_mode, tty::IsTty};
use lazy_static::lazy_static;

use crate::{
    config::arch_config::WordType,
    device::{
        cli_uart::{CliUart, CliUartHandle},
        config::Device,
        uart::Uart16550,
    },
    handle_trait::HandleTrait,
    utils::UnsignedInteger,
};
pub mod cli_uart;
mod config;
pub mod mmio;
pub mod uart;

pub trait Mem {
    fn read<T>(&mut self, addr: WordType) -> T
    where
        T: UnsignedInteger;

    fn write<T>(&mut self, addr: WordType, data: T)
    where
        T: UnsignedInteger;
}

// Check align requirement before device.read/write. Most of align requirement was checked in mmio.
pub trait DeviceTrait: Mem {
    fn one_shot(&mut self);
}

// / Peripheral initialization
lazy_static! {
    pub static ref UART1: Arc<Mutex<Device>> = Arc::new(Mutex::new(Device::Uart16550(
        Uart16550::new(0 as *const u8)
    )));
    pub static ref CLI_UART: Mutex<CliUart> = Mutex::new(CliUart::new(0 as *const u8));
}

pub fn peripheral_init() -> Vec<Box<dyn HandleTrait>> {
    if std::io::stdin().is_tty() {
        enable_raw_mode().unwrap();
    }
    // Uart
    if let Ok(mut device_guard) = UART1.lock() {
        if let Device::Uart16550(uart) = &mut *device_guard {
            let cli_inner_uart = &mut CLI_UART.lock().unwrap().uart;
            uart.change_rx_wiring(cli_inner_uart.get_tx_wiring());
            cli_inner_uart.change_rx_wiring(uart.get_tx_wiring());
        }
    }

    return vec![Box::new(CliUartHandle {})];
}
