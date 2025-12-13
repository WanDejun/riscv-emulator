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
    DeviceConfig, Emulator, EmulatorConfigurator, board::virt::VirtBoard,
    device::fast_uart::virtual_io::SerialDestination, isa::riscv::RiscvTypes,
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

fn display_device_list(devices: &Vec<DeviceConfig>) {
    println!("\x1b[{}mdevice list:", 34);
    for device in devices {
        println!("\t{:#?}: {:#?}", device.dev_type, device.path);
    }
    println!("\x1b[0m");
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

    /// Script file for debugger REPL, will be ignored if --debug is not set.
    #[arg(short = 'S', long = "script")]
    script: Option<std::path::PathBuf>,

    /// Enable to print more details.
    #[arg(short, long, default_value_t = false)]
    verbose: bool,

    /// Switch log level.
    #[arg(value_enum, long = "loglevel", default_value_t = LogLevel::Info)]
    log_level: LogLevel,

    /// Choose serial io destination.
    #[arg(value_enum, long = "serial", default_value_t = SerialDestination::Stdio)]
    serial_destination: SerialDestination,

    /// Add devices to emulator. Example: --device=virtio-block:./tmp/img_blk
    #[arg(long = "device", action = clap::ArgAction::Append)]
    devices: Vec<DeviceConfig>,
}

fn main() {
    display_welcome_message();

    if cli_args.verbose {
        println!(
            "path = {:?}, debug = {}, verbose = {}, log_level = {:?}.\r",
            cli_args.path, cli_args.debug, cli_args.verbose, cli_args.log_level
        );
        display_device_list(&cli_args.devices);
    }

    // Init emulator configuration by cli_args.
    let mut emu_cfg = EmulatorConfigurator::new();
    emu_cfg = emu_cfg.set_serial_destination(cli_args.serial_destination);
    for device in cli_args.devices.iter() {
        emu_cfg = emu_cfg.append_device(device.clone())
    }
    drop(emu_cfg);

    let _logger_handle = logging::init(cli_args.log_level);

    let ext = cli_args
        .path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("<unknown>");

    let mut board = match (cli_args.format, ext) {
        (TargetFormat::Elf, _) | (TargetFormat::Auto, "elf") => {
            if cli_args.verbose {
                println!("ELF file detected\r");
            }
            let bytes = std::fs::read(cli_args.path.clone()).expect("Failed to read target file");
            VirtBoard::from_elf(&bytes)
        }

        (TargetFormat::Bin, _) | (TargetFormat::Auto, "bin") => {
            if cli_args.verbose {
                println!("Binary file detected\r");
            }
            let bytes = std::fs::read(cli_args.path.clone()).expect("Failed to read target file");
            VirtBoard::from_binary(&bytes)
        }

        _ => {
            log::error!("Format is not supported at present.");
            panic!();
        }
    };

    if cli_args.debug {
        let mut repl = DebugREPL::<RiscvTypes>::new(&mut board);
        if let Some(script) = &cli_args.script {
            let script_content = std::fs::read_to_string(script).unwrap();
            let lines: Vec<String> = script_content.lines().map(|s| s.to_string()).collect();
            repl.run_script(&lines);
        }
        repl.run();
    } else {
        let now = Instant::now();
        let mut emulator = Emulator::from_board(board);
        match emulator.run() {
            Ok(()) => {}
            Err(e) => {
                log::error!("Error occurred while running emulator: {:?}\r", e);
            }
        }
        println!("Used time: {}s", now.elapsed().as_secs_f32());
    }
}
