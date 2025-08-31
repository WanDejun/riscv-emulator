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
    board::virt::VirtBoard,
    device::{fast_uart::virtual_io::SerialDestination, peripheral_init},
    isa::riscv::RiscvTypes,
};

use crate::{dbg_repl::DebugREPL, logging::LogLevel, welcome::display_welcome_message};

lazy_static! {
    static ref cli_args: Args = Args::parse();
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, clap::ValueEnum)]
enum TargetFormat {
    Auto,
    Elf,
    Bin,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path of the target executable file (elf/bin).
    path: std::path::PathBuf,

    /// Specify target executable file format.
    #[arg(value_enum, short, long, default_value_t = TargetFormat::Auto)]
    format: TargetFormat,

    /// Enable debugger REPL.
    #[arg(short = 'g', long = "debug", default_value_t = false)]
    debug: bool,

    /// Enable to print more details.
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

    let mut board = match (
        cli_args.format,
        cli_args.path.extension() == Some("elf".as_ref()),
    ) {
        (TargetFormat::Elf, _) | (TargetFormat::Auto, true) => {
            println!("ELF file detected\r");
            let bytes = std::fs::read(cli_args.path.clone()).unwrap();
            VirtBoard::from_binary(&bytes)
        }
        _ => {
            println!("Non-ELF file detected\r");
            todo!();
        }
    };

    if cli_args.debug {
        DebugREPL::<RiscvTypes>::new(&mut board).run();
    } else {
        let now = Instant::now();
        let emulator = Emulator::from_board(board);
        match emulator.run() {
            Ok(()) => {}
            Err(e) => {
                eprintln!("Error occurred while running emulator: {:?}\r", e);
            }
        }
        println!("Used time: {}s", now.elapsed().as_secs_f32());
    }
}
