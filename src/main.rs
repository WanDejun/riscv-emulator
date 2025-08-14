#![cfg_attr(debug_assertions, allow(dead_code))]
#![allow(incomplete_features)]
#![feature(generic_const_exprs)]
#![feature(macro_metavar_expr_concat)]

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
mod welcome;

use clap::Parser;
pub use config::ram_config;
use lazy_static::lazy_static;

use crate::{
    device::{Mem, POWER_MANAGER, peripheral_init, power_manager::POWER_OFF_CODE},
    handle_trait::HandleTrait,
    isa::riscv32,
    logging::LogLevel,
    ram::Ram,
    vaddr::VirtAddrManager,
    welcome::display_welcome_message,
};

lazy_static! {
    static ref cli_args: Args = Args::parse();
}

fn init() -> Vec<Box<dyn HandleTrait>> {
    display_welcome_message();
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

    #[arg(value_enum, long = "loglevel", default_value_t = LogLevel::Info)]
    log_level: LogLevel,
}

fn main() {
    let _logger_handle = logging::init(cli_args.log_level);
    let _init_handle = init();

    println!(
        "path = {:?}, debug = {}, verbose = {}.\r",
        cli_args.path, cli_args.debug, cli_args.verbose
    );

    let mut ram = Ram::new();

    if cli_args.path.extension() == Some("elf".as_ref()) {
        println!("ELF file detected\r");

        let bytes = std::fs::read(&cli_args.path).unwrap();
        load::load_elf(&mut ram, &bytes);
    } else {
        println!("Non-ELF file detected\r");
        todo!();
    }

    let mut cpu = riscv32::executor::RV32CPU::from_memory(VirtAddrManager::from_ram(ram));

    let mut inst_cnt = 0;
    loop {
        if let Err(e) = cpu.step() {
            eprintln!("Error executing instruction: {:?}", e);
            break;
        }
        if POWER_MANAGER
            .lock()
            .unwrap()
            .read::<u16>(0)
            .eq(&POWER_OFF_CODE)
        {
            // disable_raw_mode().unwrap();
            break;
        }

        inst_cnt += 1;
    }

    println!("Executed {} instructions.\r", inst_cnt);
}
