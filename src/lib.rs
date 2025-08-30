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

pub mod board;
pub mod cli_coordinator;
pub mod config;
pub mod device;
pub mod handle_trait;
pub mod isa;
pub mod load;

pub use config::ram_config;
use lazy_static::lazy_static;

use crate::{
    board::{Board, BoardStatus, virt::VirtBoard},
    device::fast_uart::virtual_io::SerialDestination,
    isa::riscv::{executor::RV32CPU, trap::Exception},
};
use std::{
    path::Path,
    sync::{Mutex, MutexGuard},
};

pub(crate) struct EmulatorConfig {
    pub(crate) serial_destination: SerialDestination,
}
impl EmulatorConfig {
    pub fn new() -> Self {
        Self {
            serial_destination: SerialDestination::Stdio,
        }
    }
}
lazy_static! {
    pub(crate) static ref EMULATOR_CONFIG: Mutex<EmulatorConfig> =
        Mutex::new(EmulatorConfig::new());
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
}

pub struct Emulator {
    board: VirtBoard,
}

impl Emulator {
    pub fn from_elf(path: &Path) -> Self {
        let bytes = std::fs::read(path).unwrap();
        Self {
            board: VirtBoard::from_elf(&bytes),
        }
    }

    pub fn from_board(board: VirtBoard) -> Self {
        Self { board }
    }

    pub fn run(self) -> Result<(), Exception> {
        self.run_until(&mut |_, _| false)
    }

    pub fn run_until<F>(mut self, f: &mut F) -> Result<(), Exception>
    where
        F: FnMut(&mut RV32CPU, usize) -> bool,
    {
        while self.board.status() != BoardStatus::Halt {
            self.board.step_and_halt_if(f)?;
        }

        Ok(())
    }
}
