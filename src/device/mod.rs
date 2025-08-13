use lazy_static::lazy_static;
use std::sync::{Arc, Mutex};

#[cfg(not(test))]
use crate::device::cli_uart::CliUart;
#[cfg(test)]
use crate::device::cli_uart::FIFOUart;

use crate::{
    config::arch_config::WordType,
    device::{
        cli_uart::CliUartHandle, config::Device, power_manager::PowerManager, uart::Uart16550,
    },
    handle_trait::HandleTrait,
    utils::UnsignedInteger,
};
pub mod cli_uart;
mod config;
pub mod mmio;
pub mod power_manager;
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

#[cfg(test)]
type DebugUart = FIFOUart;
#[cfg(not(test))]
type DebugUart = CliUart;

// / Peripheral initialization
lazy_static! {
    pub static ref UART1: Arc<Mutex<Device>> = Arc::new(Mutex::new(Device::Uart16550(
        Uart16550::new(0 as *const u8)
    )));
    pub static ref DEBUG_UART: Mutex<DebugUart> = Mutex::new(DebugUart::new(0 as *const u8));
    pub static ref POWER_MANAGER: Arc<Mutex<Device>> =
        Arc::new(Mutex::new(Device::PowerManager(PowerManager::new())));
}

pub fn peripheral_init() -> Vec<Box<dyn HandleTrait>> {
    // Uart
    if let Ok(mut device_guard) = UART1.lock() {
        if let Device::Uart16550(uart) = &mut *device_guard {
            let cli_inner_uart = &mut DEBUG_UART.lock().unwrap().uart;
            uart.change_rx_wiring(cli_inner_uart.get_tx_wiring());
            cli_inner_uart.change_rx_wiring(uart.get_tx_wiring());
        }
    }

    return vec![Box::new(CliUartHandle::new())];
}
