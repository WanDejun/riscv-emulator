#![cfg_attr(debug_assertions, allow(dead_code))]
#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

mod logging;
mod rvdb;
mod welcome;

use std::fs;
use std::time::Instant;

use clap::Parser;
use lazy_static::lazy_static;
use riscv_emulator::board::Board;
use riscv_emulator::isa::DebugTarget;
use riscv_emulator::isa::riscv::debugger::Address;
use riscv_emulator::{
    DeviceConfig, EmulatorConfigurator, board::virt::VirtBoard,
    device::fast_uart::virtual_io::SerialDestination,
};

use crate::{logging::LogLevel, rvdb::DebugREPL, welcome::display_welcome_message};

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

    /// Dump RISC-V arch-test signature into this file on exit.
    #[arg(long = "signature")]
    signature: Option<std::path::PathBuf>,

    /// Signature granularity in bytes (4 or 8).
    #[arg(long = "signature-granularity", default_value_t = 4)]
    signature_granularity: u32,

    /// Maximum cycles to execute before aborting (0 means no limit).
    #[arg(long = "max-cycles", default_value_t = 0)]
    max_cycles: u64,
}

fn dump_signature(
    board: &mut VirtBoard,
    out_path: &std::path::Path,
    granularity: u32,
) -> Result<(), String> {
    let loader = board
        .loader()
        .ok_or_else(|| "ELF loader not available; cannot resolve signature symbols".to_string())?;

    let symtab = loader.get_symbol_table().ok_or_else(|| {
        "No .symtab found in ELF; cannot resolve begin_signature/end_signature".to_string()
    })?;

    let begin = symtab
        .func_addr_by_name("begin_signature")
        .ok_or_else(|| "Symbol begin_signature not found".to_string())?;
    let end = symtab
        .func_addr_by_name("end_signature")
        .ok_or_else(|| "Symbol end_signature not found".to_string())?;

    if end <= begin {
        return Err(format!(
            "Invalid signature range: begin=0x{:x}, end=0x{:x}",
            begin, end
        ));
    }

    let size = end - begin;
    let step = match granularity {
        4 => 4u64,
        8 => 8u64,
        other => return Err(format!("Unsupported signature granularity: {}", other)),
    };

    if size % step != 0 {
        return Err(format!(
            "Signature size 0x{:x} not aligned to granularity {}",
            size, step
        ));
    }

    let file = std::fs::File::create(out_path).map_err(|e| {
        format!(
            "Failed to create signature file {}: {}",
            out_path.display(),
            e
        )
    })?;
    let mut w = std::io::BufWriter::new(file);

    let mut addr = begin;
    while addr < end {
        match step {
            4 => {
                let v = board
                    .cpu
                    .read_memory::<u32>(Address::Phys(addr))
                    .map_err(|e| format!("Failed to read signature @0x{:x}: {:?}", addr, e))?;
                use std::io::Write;
                writeln!(w, "{:08x}", v)
                    .map_err(|e| format!("Failed to write signature: {}", e))?;
            }
            8 => {
                let v = board
                    .cpu
                    .read_memory::<u64>(Address::Phys(addr))
                    .map_err(|e| format!("Failed to read signature @0x{:x}: {:?}", addr, e))?;
                use std::io::Write;
                writeln!(w, "{:016x}", v)
                    .map_err(|e| format!("Failed to write signature: {}", e))?;
            }
            _ => unreachable!(),
        }
        addr += step;
    }

    Ok(())
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
            VirtBoard::from_elf(bytes)
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
        let mut repl = DebugREPL::new(&mut board);
        if let Some(script) = &cli_args.script {
            let script_content = std::fs::read_to_string(script).unwrap();
            let lines: Vec<String> = script_content.lines().map(|s| s.to_string()).collect();
            repl.run_script(&lines);
        }
        repl.run();
    } else {
        if let Some(sig_path) = &cli_args.signature {
            // Create the signature file before running the emulator to ensure the file exists even if the emulator crashes.
            fs::File::create(sig_path).expect("Failed to create signature file");
        }

        let now = Instant::now();
        loop {
            if board.status() == riscv_emulator::board::BoardStatus::Halt {
                break;
            }

            if let Err(e) = board.step() {
                log::error!("Error occurred while running emulator: {:?}\r", e);
                break;
            }

            if cli_args.max_cycles != 0 && board.clock.now() >= cli_args.max_cycles {
                log::error!("Max cycles reached: {}", cli_args.max_cycles);
                break;
            }
        }

        if let Some(sig_path) = &cli_args.signature {
            if let Err(e) = dump_signature(
                &mut board,
                sig_path.as_path(),
                cli_args.signature_granularity,
            ) {
                log::error!("Failed to dump signature: {}", e);
            }
        }
        println!("Used time: {}s", now.elapsed().as_secs_f32());
    }
}
