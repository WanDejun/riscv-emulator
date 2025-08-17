#![cfg_attr(debug_assertions, allow(dead_code))]
#![allow(incomplete_features)]
#![feature(generic_const_exprs)]
#![feature(macro_metavar_expr_concat)]

mod cpu;
mod load;
mod ram;
mod utils;

pub mod config;
pub mod device;
pub mod handle_trait;
pub mod isa;

pub use config::ram_config;

use crate::{
    device::{Mem, POWER_MANAGER, power_manager::POWER_OFF_CODE},
    isa::riscv::{executor::RV32CPU, trap::Exception, vaddr::VirtAddrManager},
    ram::Ram,
};

use std::path::Path;

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

            if POWER_MANAGER
                .lock()
                .unwrap()
                .read::<u16>(0)
                .eq(&POWER_OFF_CODE)
            {
                break;
            }

            instr_cnt += 1;
        }
        self.cpu.power_off()?;
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
