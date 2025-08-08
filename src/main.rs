#![allow(incomplete_features)]
#![feature(generic_const_exprs)]

mod config;
mod cpu;
mod load;
mod ram;

mod device;
mod logging;
mod utils;

pub use config::ram_config;

fn init() {

}

fn main() {
    init();
    let _logger_handle = logging::init();

    const A: [&'static str; 12] = gen_reg_name_list!("a", 1, 5; "b", 6, 10; "c"; "d");
    for i in 0..12 {
        if i == 11 {
            println!("{}", A[i]);
        } else {
            print!("{}, ", A[i]);
        }
    }
    log::error!("[Error] ");
    log::warn!("[Warn]   ");
    log::info!("[info]   ");
    log::debug!("[debug] ");
    log::trace!("[trace] ");
}
