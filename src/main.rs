#![cfg_attr(debug_assertions, allow(dead_code))]
#![allow(incomplete_features)]
#![feature(generic_const_exprs)]
#![feature(macro_metavar_expr_concat)]

mod dbg_repl;
mod logging;
mod welcome;

use std::time::Instant;

use clap::Parser;
use lazy_static::lazy_static;
use riscv_emulator::{
    Emulator,
    device::{fast_uart::virtual_io::SerialDestination, peripheral_init},
    isa::riscv::RiscvTypes,
};

use crate::{dbg_repl::DebugREPL, logging::LogLevel, welcome::display_welcome_message};

lazy_static! {
    static ref cli_args: Args = Args::parse();
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path of the target executable file(.elf/.bin)
    path: std::path::PathBuf,

    /// Enable debugger REPL
    #[arg(short = 'g', long = "debug", default_value_t = false)]
    debug: bool,

    /// Enable to print more details
    #[arg(short, long, default_value_t = false)]
    verbose: bool,

    /// Switch log level.
    #[arg(value_enum, long = "loglevel", default_value_t = LogLevel::Info)]
    log_level: LogLevel,

    /// Choose serial io destination.
    #[arg(value_enum, long = "serial", default_value_t = SerialDestination::Stdio)]
    serial_destination: SerialDestination,
}

fn main() {
    display_welcome_message();
    let _logger_handle = logging::init(cli_args.log_level);
    let _init_handle = peripheral_init();
    // EmulatorConfigurator::new().set_serial_destination(cli_args.serial_destination);

    println!(
        "path = {:?}, debug = {}, verbose = {}.\r",
        cli_args.path, cli_args.debug, cli_args.verbose
    );

    let mut emulator = if cli_args.path.extension() == Some("elf".as_ref()) {
        println!("ELF file detected\r");
        Emulator::from_elf(&cli_args.path)
    } else {
        println!("Non-ELF file detected\r");
        todo!();
    };

    if cli_args.debug {
        DebugREPL::<RiscvTypes>::new(emulator.into()).run();
    } else {
        let now = Instant::now();
        match emulator.run() {
            Ok(cnt) => {
                println!("Executed {} instructions.\r", cnt);
            }
            Err(e) => {
                eprintln!("Error occurred while running emulator: {:?}\r", e);
            }
        }
        println!("Used time: {}s", now.elapsed().as_secs_f32());
    }
}
