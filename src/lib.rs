#![cfg_attr(debug_assertions, allow(dead_code))]
#![allow(incomplete_features)]
#![feature(generic_const_exprs)]
#![feature(macro_metavar_expr_concat)]
#![feature(cold_path)]
#![feature(likely_unlikely)]

mod cpu;
mod load;
mod ram;
mod utils;

pub mod cli_coordinator;
pub mod config;
pub mod device;
pub mod handle_trait;
pub mod isa;

pub use config::ram_config;

use crate::{
    device::power_manager::{POWER_OFF_CODE, POWER_STATUS},
    isa::riscv::{executor::RV32CPU, trap::Exception, vaddr::VirtAddrManager},
    ram::Ram,
};

use std::{hint::cold_path, path::Path};

pub struct Emulator {
    cpu: RV32CPU,
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
        let mut instr_cnt: usize = 0;

        loop {
            self.cpu.step()?;
            let power = POWER_STATUS.load(std::sync::atomic::Ordering::Acquire);

            if instr_cnt % 32 == 0 && power.eq(&POWER_OFF_CODE) {
                cold_path();
                self.cpu.power_off()?;
                break;
            }

            instr_cnt += 1;
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
