#![cfg_attr(debug_assertions, allow(dead_code))]
#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

mod config;
mod cpu;
mod load;
mod ram;
mod vaddr;

mod device;
mod handle_trait;
mod isa;
mod logging;
mod utils;

use clap::Parser;
pub use config::ram_config;
use lazy_static::lazy_static;

use crate::{
    device::{peripheral_init, DeviceTrait, DEBUG_UART, UART1}, handle_trait::HandleTrait, isa::riscv32, logging::LogLevel, ram::Ram, vaddr::VirtAddrManager
};

lazy_static! {
    static ref cli_args: Args = Args::parse();
}

fn init() -> Vec<Box<dyn HandleTrait>> {
    let mut handles = vec![];
    let peripheral_handle = peripheral_init();
    for handle in peripheral_handle {
        handles.push(handle);
    }

    handles
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path of the target executable file(.elf/.bin)
    path: std::path::PathBuf,

    /// enable debug
    #[arg(short = 'g', long = "debug", default_value_t = false)]
    debug: bool,

    /// Enable to print more details
    #[arg(short, long, default_value_t = false)]
    verbose: bool,

    #[arg(value_enum, default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

fn main() {
    let _logger_handle = logging::init(cli_args.log_level);
    let _init_handle = init();

    println!(
        "path = {:?}, debug = {}, verbose = {}.",
        cli_args.path, cli_args.debug, cli_args.verbose
    );

    let mut ram = Ram::new();

    if cli_args.path.extension() == Some("elf".as_ref()) {
        println!("ELF file detected");

        let bytes = std::fs::read(&cli_args.path).unwrap();
        load::load_elf(&mut ram, &bytes);
    } else {
        println!("Non-ELF file detected");
        todo!();
    }

    let mut cpu = riscv32::executor::RV32CPU::from_memory(VirtAddrManager::from_ram(ram));

    let mut inst_cnt = 0;
    loop {
        if let Err(e) = cpu.step() {
            eprintln!("Error executing instruction: {:?}", e);
            break;
        }

        inst_cnt += 1;
        UART1.lock().unwrap().one_shot();
        DEBUG_UART.lock().unwrap().one_shot();
    }

    println!("Executed {} instructions.", inst_cnt);
}
