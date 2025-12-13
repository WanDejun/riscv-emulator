#![cfg_attr(debug_assertions, allow(dead_code))]
#![allow(incomplete_features)]
#![feature(generic_const_exprs)]
#![feature(macro_metavar_expr_concat)]
#![feature(cold_path)]
#![feature(likely_unlikely)]
#![feature(unsafe_cell_access)]

mod cpu;
mod fpu;
mod ram;
mod utils;
mod vclock;

pub mod async_poller;
pub mod board;
pub mod cli_coordinator;
pub mod config;
pub mod device;
pub mod isa;
pub mod load;

pub use config::ram_config;
use lazy_static::lazy_static;

use crate::{
    board::{Board, BoardStatus, virt::VirtBoard},
    device::{fast_uart::virtual_io::SerialDestination, virtio::virtio_mmio::VirtIODeviceID},
    isa::riscv::trap::Exception,
};
use std::{
    path::{Path, PathBuf},
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
            None => return Err("Invaild device arguments.".into()),
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
    pub fn from_binary(path: &Path) -> Self {
        let bytes = std::fs::read(path).unwrap();
        Self {
            board: VirtBoard::from_binary(&bytes),
        }
    }

    pub fn from_elf(path: &Path) -> Self {
        let bytes = std::fs::read(path).unwrap();
        Self {
            board: VirtBoard::from_elf(&bytes),
        }
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
}
