#![cfg_attr(debug_assertions, allow(dead_code))]
#![allow(incomplete_features)]
#![feature(generic_const_exprs)]
#![feature(macro_metavar_expr_concat)]
#![feature(likely_unlikely)]
#![feature(unsafe_cell_access)]

#[cfg(all(feature = "native-cli", target_arch = "wasm32"))]
compile_error!("feature 'native-cli' is not supported on wasm32 targets");

#[cfg(all(feature = "web", not(target_arch = "wasm32")))]
compile_error!("feature 'web' requires wasm32 target");

mod cpu;
mod fpu;
mod utils;
mod vclock;

pub mod board;
pub mod cli_coordinator;
pub mod config;
pub mod device;
pub mod device_poller;
pub mod isa;
pub mod load;
pub mod ram;

#[cfg(feature = "web")]
pub mod wasm_api;

pub use config::ram_config;
use lazy_static::lazy_static;

use crate::{
    board::{Board, BoardStatus, virt::VirtBoard},
    device::{fast_uart::virtual_io::SerialDestination, virtio::virtio_mmio::VirtIODeviceID},
    isa::riscv::trap::Exception,
};
use std::{
    path::PathBuf,
    str::FromStr,
    sync::{Mutex, MutexGuard},
};

#[derive(Debug, Clone)]
pub struct DeviceConfig {
    pub dev_type: VirtIODeviceID,
    pub path: PathBuf,
}

impl FromStr for DeviceConfig {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split(':');
        let dev_type = match parts.next() {
            Some("virtio-block") => VirtIODeviceID::Block,
            Some("virtio-network") => VirtIODeviceID::Network,
            Some(other) => return Err(format!("Unknown device type: {}", other)),
            None => return Err("Invalid device arguments.".into()),
        };
        let path = PathBuf::from(parts.next().ok_or("Need input a device path.")?);
        Ok(DeviceConfig { dev_type, path })
    }
}

pub struct EmulatorConfig {
    pub(crate) serial_destination: SerialDestination,
    pub(crate) devices: Vec<DeviceConfig>,
}
impl EmulatorConfig {
    pub fn new() -> Self {
        Self {
            serial_destination: SerialDestination::Test,
            devices: vec![],
        }
    }
}
lazy_static! {
    pub static ref EMULATOR_CONFIG: Mutex<EmulatorConfig> = Mutex::new(EmulatorConfig::new());
}

pub struct EmulatorConfigurator<'a> {
    lock: MutexGuard<'a, EmulatorConfig>,
}
impl<'a> EmulatorConfigurator<'a> {
    pub fn new() -> Self {
        Self {
            lock: EMULATOR_CONFIG.lock().unwrap(),
        }
    }
    pub fn set_serial_destination(mut self, new_destination: SerialDestination) -> Self {
        self.lock.serial_destination = new_destination;
        self
    }

    pub fn append_device(mut self, device: DeviceConfig) -> Self {
        self.lock.devices.push(device);
        self
    }
}

pub struct Emulator {
    board: VirtBoard,
}

impl Emulator {
    pub fn from_binary_bytes(bytes: &[u8]) -> Self {
        Self {
            board: VirtBoard::from_binary(bytes),
        }
    }

    pub fn try_from_elf_bytes(bytes: Vec<u8>) -> Result<Self, String> {
        Ok(Self {
            board: VirtBoard::try_from_elf(bytes)?,
        })
    }

    pub fn from_board(board: VirtBoard) -> Self {
        Self { board }
    }

    pub fn run(&mut self) -> Result<(), Exception> {
        while self.board.status() != BoardStatus::Halt {
            self.board.step()?;
        }

        Ok(())
    }

    pub fn step(&mut self) -> Result<(), Exception> {
        if self.board.status() != BoardStatus::Halt {
            self.board.step()?;
        }
        Ok(())
    }

    pub fn run_steps(&mut self, max_steps: u64) -> Result<u64, Exception> {
        let mut steps = 0;
        while self.board.status() != BoardStatus::Halt && steps < max_steps {
            self.board.step()?;
            steps += 1;
        }
        Ok(steps)
    }

    pub fn board(&self) -> &VirtBoard {
        &self.board
    }

    pub fn board_mut(&mut self) -> &mut VirtBoard {
        &mut self.board
    }

    #[cfg(feature = "web")]
    pub fn push_uart_input_bytes(&self, bytes: &[u8]) {
        self.board.push_uart_input(bytes);
    }

    #[cfg(feature = "web")]
    pub fn take_uart_output_bytes(&self) -> Vec<u8> {
        self.board.take_uart_output()
    }
}
