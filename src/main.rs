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
use flexi_logger::LoggerHandle;
use lazy_static::lazy_static;

use crate::{device::peripheral_init, handle_trait::HandleTrait};

lazy_static! {
    static ref cli_args: Args = Args::parse();
    static ref _logger_handle: LoggerHandle = logging::init();
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
}

fn main() {
    const A: [&'static str; 12] = gen_reg_name_list!("a", 1, 5; "b", 6, 10; "c"; "d");
    for i in 0..12 {
        if i == 11 {
            println!("{}", A[i]);
        } else {
            print!("{}, ", A[i]);
        }
    }
    
    let _init_handle = init();

    println!(
        "path = {:?}, debug = {}, verbose = {}.",
        cli_args.path, cli_args.debug, cli_args.verbose
    );
}
