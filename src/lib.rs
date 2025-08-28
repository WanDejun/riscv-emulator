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

pub mod cli_coordinator;
pub mod config;
pub mod device;
pub mod handle_trait;
pub mod isa;
pub mod load;

pub use config::ram_config;
use lazy_static::lazy_static;

use crate::{
    device::{
        fast_uart::virtual_io::SerialDestination,
        power_manager::{POWER_OFF_CODE, POWER_STATUS},
    },
    isa::riscv::{executor::RV32CPU, mmu::VirtAddrManager, trap::Exception},
    ram::Ram,
};

use std::{
    hint::cold_path,
    path::Path,
    sync::atomic::Ordering,
    sync::{Mutex, MutexGuard},
};

pub struct Emulator {
    cpu: RV32CPU,
}

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

impl Emulator {
    pub fn from_elf(path: &Path) -> Self {
        let mut ram = Ram::new();
        let bytes = std::fs::read(path).unwrap();
        load::load_elf(&mut ram, &bytes);
        Self {
            cpu: RV32CPU::from_memory(VirtAddrManager::from_ram(ram)),
        }
    }

    pub fn run(&mut self) -> Result<usize, Exception> {
        self.run_until(|_cpu, _instr| false) // Do nothing
    }

    /// Invoke `f` after each CPU step.
    /// `f` is called as with &mut CPU and instruction count.
    /// If `f` returns `true`, the emulator will power off.
    pub fn run_until(
        &mut self,
        mut f: impl FnMut(&mut RV32CPU, usize) -> bool,
    ) -> Result<usize, Exception> {
        let mut instr_cnt: usize = 0;
        POWER_STATUS.store(0, Ordering::Release);

        loop {
            self.cpu.step()?;
            instr_cnt += 1;

            if instr_cnt % 32 == 0 && POWER_STATUS.load(Ordering::Acquire).eq(&POWER_OFF_CODE)
                || f(&mut self.cpu, instr_cnt)
            {
                cold_path();
                self.cpu.power_off()?;
                log::debug!("iCache hit for {} times.", self.cpu.icache_cnt);
                let rate = self.cpu.icache_cnt as f64 / instr_cnt as f64;
                log::debug!("iCache hit rate {}", rate);
                break;
            }
        }

        Ok(instr_cnt)
    }

    pub fn cpu_mut(&mut self) -> &mut RV32CPU {
        &mut self.cpu
    }
}

impl Into<RV32CPU> for Emulator {
    fn into(self) -> RV32CPU {
        self.cpu
    }
}
