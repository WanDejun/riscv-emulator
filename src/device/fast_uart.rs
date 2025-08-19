//! Only realize the basic functions of Uart16550J: TX, RX for 8bits data, 2/1stop bits.
//! # TODO
//! - [ ] interrupt
//! - [ ] FIFO
//! - [ ] DMA support
//! - [ ] Even/Odd Parity
//! - [ ] Different length of data bits;

#![allow(unused)]

use std::{
    cell::RefCell,
    io::{self, Write},
    sync::{Arc, WaitTimeoutResult, atomic::AtomicBool},
    thread,
    time::Duration,
    u8,
};

use crossbeam::channel::{self, Receiver, Sender};
use crossterm::event::{self, Event, KeyCode};
#[cfg(not(test))]
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use log::error;

use crate::{
    cli_coordinator::CliCoordinator,
    config::arch_config::WordType,
    device::{DeviceTrait, Mem, MemError, config::UART_DEFAULT_DIV},
    handle_trait::HandleTrait,
    isa::riscv::trap::Exception,
    utils::{clear_bit, read_bit, set_bit},
};

const UART_DATA_LENGTH: u8 = 8;

#[allow(unused)]
mod offset {
    use super::WordType;

    pub const RBR: WordType = 0x00;
    pub const THR: WordType = 0x00;
    pub const IER: WordType = 0x01;
    pub const IIR: WordType = 0x02;
    pub const FCR: WordType = 0x02;
    pub const LCR: WordType = 0x03;
    pub const MCR: WordType = 0x04;
    pub const LSR: WordType = 0x05;
    pub const MSR: WordType = 0x06;
    pub const SCR: WordType = 0x07;
    pub const DLL: WordType = 0x00;
    pub const DLM: WordType = 0x01;
}

#[rustfmt::skip]
#[allow(non_snake_case)]
#[repr(C)]
/// See doc/device/uart.md
struct Uart16550Reg {   //  | LCR   |  Addr |         Description               | Access Type
    RBR:    u8,         //  | 0     | +0x0  | Receiver Buffer Register          |   RO
    THR:    u8,         //  | 0     | +0x0  | Transmitter Holding Register      |   WO
    IER:    u8,         //  | 0     | +0x1  | Interrupt Enable Register         |   RW
    IIR:    u8,         //  | Any   | +0x2  | Interrupt Identification Register |   RO
    // FCR:    u8,      //  | Any   | +0x2  | FIFO Control Register             |   WO
    FCR:    u8,         //  | 1     | +0x2  | FIFO Control Register             |   RO
    LCR:    u8,         //  | Any   | +0x3  | Line Control Register             |   RW
    MCR:    u8,         //  | Any   | +0x4  | Modem Control Register            |   RW
    LSR:    u8,         //  | Any   | +0x5  | Line Status Register              |   RW
    MSR:    u8,         //  | Any   | +0x6  | Modem Status Register             |   RW
    SCR:    u8,         //  | Any   | +0x7  | Scratch Register                  |   RW
    DLL:    u8,         //  | 1     | +0x0  | Divisor Latch(low)   Register     |   RW
    DLM:    u8,         //  | 1     | +0x1  | Divisor Latch(most)  Register     |   RW
}

impl Uart16550Reg {
    fn new() -> Self {
        Self {
            RBR: 0,
            THR: 0,
            IER: 0,
            IIR: 0,
            FCR: 0,
            LCR: 0x07,
            MCR: 0,
            LSR: 0x60,
            MSR: 0,
            SCR: 0,
            DLL: UART_DEFAULT_DIV as u8,
            DLM: (UART_DEFAULT_DIV >> 8) as u8,
        }
    }

    fn get_divisor(&self) -> u16 {
        (self.DLL as u16) + ((self.DLM as u16) << 8)
    }

    fn get_tx_data(&mut self) -> Option<u8> {
        if read_bit(&self.LSR, 5) {
            None
        } else {
            set_bit(&mut self.LSR, 5);
            Some(self.THR)
        }
    }

    fn write_transmit_empty<const BIT: bool>(&mut self) {
        if BIT {
            set_bit(&mut self.LSR, 6);
        } else {
            clear_bit(&mut self.LSR, 6);
        }
    }

    fn get_stop_bits(&self) -> u8 {
        if (self.LCR & (1 << 2)) != 0 { 2 } else { 1 }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Uart16550Status {
    IDLE,
    START,
    DATA(u8, u8),
    STOP(u8),
}

#[allow(non_snake_case)]
pub struct FastUart16550 {
    reg: Arc<RefCell<Uart16550Reg>>,
    reg_ptr: [*const u8; 8],
    reg_mut_ptr: [*mut u8; 8],
    reg_lcr_ptr: [*mut u8; 8],

    input_rx: Receiver<u8>,
    output_tx: Sender<u8>,
    output_rx: Receiver<u8>,
    sync_lock: Arc<AtomicBool>,
}
impl FastUart16550 {
    pub fn new() -> Self {
        let reg = Arc::new(RefCell::new(Uart16550Reg::new()));
        let mut reg_ref = reg.borrow_mut();
        let reg_ptr = [
            (&reg_ref.RBR) as *const u8,
            (&reg_ref.IER) as *const u8,
            (&reg_ref.IIR) as *const u8,
            (&reg_ref.LCR) as *const u8,
            (&reg_ref.MCR) as *const u8,
            (&reg_ref.LSR) as *const u8,
            (&reg_ref.MSR) as *const u8,
            (&reg_ref.SCR) as *const u8,
        ];
        let reg_mut_ptr = [
            (&mut reg_ref.THR) as *mut u8,
            (&mut reg_ref.IER) as *mut u8,
            (&mut reg_ref.FCR) as *mut u8,
            (&mut reg_ref.LCR) as *mut u8,
            (&mut reg_ref.MCR) as *mut u8,
            (&mut reg_ref.LSR) as *mut u8,
            (&mut reg_ref.MSR) as *mut u8,
            (&mut reg_ref.SCR) as *mut u8,
        ];
        let reg_lcr_ptr = [
            (&mut reg_ref.DLL) as *mut u8,
            (&mut reg_ref.DLM) as *mut u8,
            (&mut reg_ref.FCR) as *mut u8,
            (&mut reg_ref.LCR) as *mut u8,
            (&mut reg_ref.MCR) as *mut u8,
            (&mut reg_ref.LSR) as *mut u8,
            (&mut reg_ref.MSR) as *mut u8,
            (&mut reg_ref.SCR) as *mut u8,
        ];

        let (input_tx, input_rx) = channel::unbounded();
        let (output_tx, output_rx) = channel::unbounded();

        let sync_lock = Arc::new(AtomicBool::new(false));

        spawn_io_thread(input_tx, output_rx.clone(), sync_lock.clone());

        drop(reg_ref);
        Self {
            reg: reg.clone(),
            reg_ptr,
            reg_mut_ptr,
            reg_lcr_ptr,
            input_rx,
            output_tx,
            output_rx,
            sync_lock,
        }
    }

    #[allow(non_snake_case)]
    fn read_RBR(&mut self) -> u8 {
        clear_bit(&mut self.reg.borrow_mut().LSR, 0); // receive data ready.
        self.reg.borrow().RBR
    }

    #[allow(non_snake_case)]
    fn write_RBR(&mut self, data: u8) {
        set_bit(&mut self.reg.borrow_mut().LSR, 0); // receive data ready.
        self.reg.borrow_mut().RBR = data
    }
}

impl Mem for FastUart16550 {
    fn read<T>(&mut self, inner_addr: WordType) -> Result<T, MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        // check terminal input.
        if let Ok(data) = self.input_rx.try_recv() {
            self.write_RBR(data)
        }

        let inner_addr: usize = inner_addr as usize;
        let size = size_of::<T>();
        debug_assert!(inner_addr as usize + size <= 8);

        let mut data: T = 0u8.into();
        if (self.reg.borrow().LCR & (1 << 7)) == (1 << 7) {
            // LCR
            for i in inner_addr..8.min(inner_addr + size) {
                data |= T::from(
                    unsafe { self.reg_lcr_ptr[i].read_volatile() } << (8 * (i - inner_addr)),
                )
            }
        } else {
            // Normal
            for i in inner_addr..8.min(inner_addr + size) {
                if i == 0 {
                    data = self.read_RBR().into(); // RBR must be the first byte.
                } else {
                    data |= T::from(
                        unsafe { self.reg_ptr[i].read_volatile() } << (8 * (i - inner_addr)),
                    );
                }
            }
        }

        Ok(data)
    }

    fn write<T>(&mut self, inner_addr: WordType, data: T) -> Result<(), MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        let inner_addr: usize = inner_addr as usize;
        let size = size_of::<T>();
        assert!(inner_addr as usize + size <= 8);
        let mut data: u64 = data.into();

        if (self.reg.borrow().LCR & (1 << 7)) == (1 << 7) {
            // LCR
            for i in inner_addr..8.min(inner_addr + size) {
                unsafe { self.reg_lcr_ptr[i].write_volatile((data & (0xff)) as u8) }
                data >>= 1;
            }
        } else {
            // Normal
            for i in inner_addr..8.min(inner_addr + size) {
                if i == 0 {
                    self.output_tx.send((data & (0xff)) as u8);
                } else {
                    unsafe { self.reg_mut_ptr[i].write_volatile((data & (0xff)) as u8) };
                }
                data >>= 1;
            }
        }

        Ok(())
    }
}

impl DeviceTrait for FastUart16550 {
    fn sync(&mut self) {
        loop {
            if self.output_rx.is_empty() {
                loop {
                    if !self.sync_lock.load(std::sync::atomic::Ordering::Acquire) {
                        break;
                    }
                }
                break;
            }
        }
    }
}

/// Set terminal to raw mode. RAII to unset terminal raw mode.
pub struct FastUart16550Handle {}
impl FastUart16550Handle {
    pub fn new() -> Self {
        #[cfg(not(test))]
        enable_raw_mode().unwrap();

        Self {}
    }
}
impl HandleTrait for FastUart16550Handle {}
impl Drop for FastUart16550Handle {
    fn drop(&mut self) {
        #[cfg(not(test))]
        disable_raw_mode().unwrap(); // 恢复终端原始状态
    }
}

fn spawn_io_thread(input_tx: Sender<u8>, output_rx: Receiver<u8>, sync_lock: Arc<AtomicBool>) {
    thread::spawn(move || {
        loop {
            CliCoordinator::global().confirm_pause_and_wait();

            // output epoll
            loop {
                // lock
                if !sync_lock.swap(true, std::sync::atomic::Ordering::AcqRel) {
                    break;
                }
            }
            while let Ok(v) = output_rx.try_recv() {
                print!("{}", v as char);
            }
            io::stdout().flush().unwrap();
            sync_lock.store(false, std::sync::atomic::Ordering::Release);

            // input epoll
            if event::poll(Duration::from_millis(20)).unwrap() {
                if let Event::Key(k) = event::read().unwrap() {
                    match k.code {
                        KeyCode::Char(c) => input_tx.send(c as u8).unwrap(),
                        KeyCode::Tab => input_tx.send(b'\t').unwrap(),
                        KeyCode::Backspace => input_tx.send(0x08).unwrap(),
                        KeyCode::Enter => input_tx.send(b'\r').unwrap(),
                        KeyCode::Up => {
                            for v in [0x1B, 0x5B, 0x41] {
                                input_tx.send(v).unwrap();
                            }
                        }
                        KeyCode::Down => {
                            for v in [0x1B, 0x5B, 0x42] {
                                input_tx.send(v).unwrap();
                            }
                        }
                        KeyCode::Left => {
                            for v in [0x1B, 0x5B, 0x43] {
                                input_tx.send(v).unwrap();
                            }
                        }
                        KeyCode::Right => {
                            for v in [0x1B, 0x5B, 0x44] {
                                input_tx.send(v).unwrap();
                            }
                        }
                        _ => {}
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    });
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    #[ignore = "io test"]
    fn output_test() {
        let mut uart = FastUart16550::new();
        let _handler = FastUart16550Handle::new();

        uart.write(0, 'a' as u8);

        uart.sync();
    }
}
