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
    sync::{Arc, WaitTimeoutResult},
    u8,
};

use log::error;

use crate::{
    config::arch_config::WordType,
    device::{DeviceTrait, Mem, MemError, config::UART_DEFAULT_DIV},
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

struct Uart16550TX {
    uart_reg: Arc<RefCell<Uart16550Reg>>,
    status: Uart16550Status,
    div_counter: u32, // count frequency. Switch output data in (DLL + (DLM << 8)) * 16 clocks;

    tx_data: Option<u8>,
    tx_reg: Box<u8>,
}
impl Uart16550TX {
    fn new(uart_reg: &Arc<RefCell<Uart16550Reg>>) -> Self {
        Self {
            uart_reg: uart_reg.clone(),
            status: Uart16550Status::IDLE,
            div_counter: 0,
            tx_reg: Box::new(1),
            tx_data: None,
        }
    }

    fn get_wire(&self) -> *const u8 {
        &*self.tx_reg
    }

    fn read_tx_data(&self) -> Option<u8> {
        self.uart_reg.borrow_mut().get_tx_data()
    }

    fn advance_state_machine(&mut self) {
        match self.status {
            Uart16550Status::START => {
                let data = self.tx_data.unwrap();
                self.status = Uart16550Status::DATA(1, data >> 1);
                *self.tx_reg = data & 0x01;
            }
            Uart16550Status::DATA(mut cnt, data) => {
                cnt += 1;
                *self.tx_reg = data & 0x01;
                if cnt > UART_DATA_LENGTH {
                    *self.tx_reg = 0x01;
                    self.status = Uart16550Status::STOP(0);
                } else {
                    self.status = Uart16550Status::DATA(cnt, data >> 1);
                }
            }
            Uart16550Status::STOP(cnt) => {
                let nxt_cnt = cnt + 1;
                if nxt_cnt == self.uart_reg.borrow().get_stop_bits() {
                    self.status = Uart16550Status::IDLE;
                } else {
                    self.status = Uart16550Status::STOP(nxt_cnt);
                }
            }
            _ => {}
        }
    }

    fn step(&mut self) {
        if self.status == Uart16550Status::IDLE {
            self.tx_data = self.read_tx_data();
            if self.tx_data.is_some() {
                self.status = Uart16550Status::START;
                *self.tx_reg = 0;
                self.uart_reg.borrow_mut().write_transmit_empty::<false>();
            } else {
                self.uart_reg.borrow_mut().write_transmit_empty::<true>();
            }
        }

        if self.status == Uart16550Status::IDLE {
            return;
        }

        self.div_counter += 1;
        if self.div_counter < ((self.uart_reg.borrow().get_divisor() as u32) << 4) {
            return;
        }
        self.div_counter = 0;

        self.advance_state_machine();
    }
}

struct Uart16550RX {
    uart_reg: Arc<RefCell<Uart16550Reg>>,
    status: Uart16550Status,
    div_counter: u16, // count frequency. Take one sample in DLL + (DLM << 8) clocks;
    sample_data: u8,  // Increasing when get high bit.
    rx_wiring: *const u8,

    sample_count: u8, // 16 times sampling for a bit
}
impl Uart16550RX {
    fn new(uart_reg: &Arc<RefCell<Uart16550Reg>>, rx_wiring: *const u8) -> Self {
        Self {
            uart_reg: uart_reg.clone(),
            status: Uart16550Status::IDLE,
            div_counter: 0,
            sample_data: 0,
            sample_count: 0,
            rx_wiring,
        }
    }

    fn write_data2reg(&self, data: u8) {
        self.uart_reg.borrow_mut().RBR = data;
        set_bit(&mut self.uart_reg.borrow_mut().LSR, 0); // set receive data ready.
    }

    fn advance_idle_status(&mut self) {
        // TODO: add pre_sample_data to filter
        if self.sample_data == 0 && self.sample_count == 1 {
            self.status = Uart16550Status::START;
        } else {
            self.sample_data = 0;
            self.sample_count = 0;
        }
    }

    // Push the state machine back one bit
    fn advance_state_machine(&mut self) {
        let mut bit_data = false;
        // Processing sampling. We can aslo processing noisy signals here.
        if self.sample_data > 12 {
            bit_data = true;
        } else if self.sample_data < 5 {
            bit_data = false;
        } else {
            // noice
        }

        match self.status {
            Uart16550Status::START => {
                if bit_data {
                    self.status = Uart16550Status::IDLE; // False Start
                } else {
                    self.status = Uart16550Status::DATA(0, 0);
                }
            }
            Uart16550Status::DATA(mut cnt, cur) => {
                let mut nxt = cur;
                if bit_data {
                    nxt |= 1 << cnt;
                }
                cnt += 1;

                if cnt == UART_DATA_LENGTH {
                    self.write_data2reg(nxt);
                    self.status = Uart16550Status::IDLE;
                } else {
                    self.status = Uart16550Status::DATA(cnt, nxt);
                }
            }
            _ => {
                error!("illigal status in RXD: {:?}", self.status);
                panic!("illigal status!");
            }
        }
    }

    fn take_sample(&mut self) {
        self.sample_count += 1;

        let tx_level = unsafe { self.rx_wiring.read_volatile() };
        if tx_level != 0 {
            self.sample_data += 1;
        }
    }

    fn step(&mut self) {
        let divisor = self.uart_reg.borrow().get_divisor(); // DLL + (DLM << 8)

        self.div_counter += 1;
        if self.div_counter < divisor {
            return;
        }
        self.div_counter = 0;

        self.take_sample();
        if self.status == Uart16550Status::IDLE {
            self.advance_idle_status();
            return;
        }
        // `16` is the number of samples
        if self.sample_count < 16 {
            return;
        }
        self.advance_state_machine();
        self.sample_count = 0;
        self.sample_data = 0;
    }

    pub fn change_rx_wiring(&mut self, rx_wiring: *const u8) {
        self.rx_wiring = rx_wiring
    }
}

unsafe impl Send for Uart16550 {}
#[allow(non_snake_case)]
pub struct Uart16550 {
    reg: Arc<RefCell<Uart16550Reg>>,
    reg_ptr: [*const u8; 8],
    reg_mut_ptr: [*mut u8; 8],
    reg_lcr_ptr: [*mut u8; 8],

    rx: Uart16550RX,
    tx: Uart16550TX,
}
impl Uart16550 {
    pub fn new(rx_wiring: *const u8) -> Self {
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

        drop(reg_ref);
        Self {
            reg: reg.clone(),
            reg_ptr,
            reg_mut_ptr,
            reg_lcr_ptr,
            rx: Uart16550RX::new(&reg, rx_wiring),
            tx: Uart16550TX::new(&reg),
        }
    }

    #[allow(non_snake_case)]
    fn read_RBR(&mut self) -> u8 {
        clear_bit(&mut self.reg.borrow_mut().LSR, 0); // receive data ready.
        self.reg.borrow().RBR
    }

    #[allow(non_snake_case)]
    fn write_THR(&mut self, tx_data: u8) {
        clear_bit(&mut self.reg.borrow_mut().LSR, 5); // transmit empty
        self.reg.borrow_mut().THR = tx_data;
    }

    pub fn change_rx_wiring(&mut self, rx_wiring: *const u8) {
        self.rx.change_rx_wiring(rx_wiring);
    }

    pub fn get_tx_wiring(&self) -> *const u8 {
        self.tx.get_wire()
    }

    pub fn transmit_holding_empty(&self) -> bool {
        (self.reg.borrow().LSR & (1 << 5)) == 0
    }
}

impl Mem for Uart16550 {
    fn read<T>(&mut self, inner_addr: WordType) -> Result<T, MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        let inner_addr: usize = inner_addr as usize;
        let size = size_of::<T>();
        assert!(inner_addr as usize + size <= 8);

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
                    self.write_THR((data & (0xff)) as u8);
                } else {
                    unsafe { self.reg_mut_ptr[i].write_volatile((data & (0xff)) as u8) };
                }
                data >>= 1;
            }
        }

        Ok(())
    }
}
impl DeviceTrait for Uart16550 {
    fn step(&mut self) {
        self.tx.step();
        self.rx.step();
    }
    fn sync(&mut self) {
        while !(read_bit(&self.reg.borrow().LSR, 6)) {
            self.step();
        }
    }
}

#[cfg(test)]
mod test {
    use std::{cell::RefCell, sync::Arc};

    use log::error;

    use crate::{
        device::{
            DeviceTrait, Mem,
            config::UART_DEFAULT_DIV,
            uart::{
                self, Uart16550, Uart16550RX, Uart16550Reg, Uart16550TX,
                offset::{self, THR},
            },
        },
        utils::{clear_bit, set_bit},
        *,
    };

    #[test]
    fn rx_test() {
        let uart_reg = Arc::new(RefCell::new(Uart16550Reg::new()));
        let mut rx_wiring: u8 = 1;
        let mut rx = Uart16550RX::new(&uart_reg, (&rx_wiring) as *const u8);

        uart_reg.borrow_mut().DLL = 0xe8;
        uart_reg.borrow_mut().DLM = 0x03;

        for _ in 0..10000 {
            rx.step();
        }

        assert!(uart_reg.borrow_mut().DLL == 0xe8);
        assert!(uart_reg.borrow_mut().DLM == 0x03);
        assert!(uart_reg.borrow_mut().LSR == 0x60);

        let data: [u8; 14] = [1, 1, 0x00, 1, 0, 1, 0, 1, 0, 1, 0, 1, 1, 1]; // 0x55
        for data_bit in data {
            for _ in 0..1000 * 16 {
                rx_wiring = data_bit;
                rx.step();
            }
        }
        assert!(uart_reg.borrow_mut().LSR & 0x01 != 0);
        assert!(uart_reg.borrow_mut().RBR == 0x55);
    }

    #[test]
    fn tx_test() {
        let uart_reg = Arc::new(RefCell::new(Uart16550Reg::new()));
        let mut tx = Uart16550TX::new(&uart_reg);
        let tx_wire = tx.get_wire();

        uart_reg.borrow_mut().DLL = 0xe8;
        uart_reg.borrow_mut().DLM = 0x03;

        uart_reg.borrow_mut().THR = 0xaa;
        clear_bit(&mut uart_reg.borrow_mut().LSR, 5);

        let mut data: u16 = 0;

        for i in 0..12 {
            for j in 0..1000 * 16 {
                tx.step();
                if (j == 8000) {
                    data |= ((unsafe { tx_wire.read_volatile() } as u16) << i);
                }
            }
            if i == 2 {
                assert!((uart_reg.borrow_mut().LSR & (1 << 5)) != 0);
                assert!((uart_reg.borrow_mut().LSR & (1 << 6)) == 0);
            }
        }
        assert!((uart_reg.borrow_mut().LSR & (1 << 6)) != 0);
        assert!(((data >> 1) & 0xff) == 0xaa);
    }

    #[test]
    fn tx_rx_test() {
        let uart_reg = Arc::new(RefCell::new(Uart16550Reg::new()));
        let mut tx = Uart16550TX::new(&uart_reg);
        let mut rx = Uart16550RX::new(&uart_reg, tx.get_wire());

        uart_reg.borrow_mut().DLL = 0xe8;
        uart_reg.borrow_mut().DLM = 0x03;
        uart_reg.borrow_mut().THR = 0xaa;
        clear_bit(&mut uart_reg.borrow_mut().LSR, 5);

        let mut data: u16 = 0;
        for i in 0..12 {
            for j in 0..1000 * 16 {
                rx.step();
                tx.step();
            }
        }
        assert!((uart_reg.borrow_mut().LSR & (1 << 6)) != 0);
        assert!(uart_reg.borrow_mut().LSR & 0x01 != 0);
        assert!(uart_reg.borrow_mut().RBR == 0xaa);
    }

    #[test]
    fn uart_test() {
        let mut uart = Uart16550::new(0 as *const u8);
        uart.change_rx_wiring(uart.tx.get_wire());

        set_bit(&mut uart.reg.borrow_mut().LCR, 7); // set LCR
        set_bit(&mut uart.reg.borrow_mut().LCR, 2); // set LCR(2 stop bits)
        // uart.write(offset::DLL, 0xe8u8); // set DLL
        // uart.write(offset::DLM, 0x03u8); // set DLM

        clear_bit(&mut uart.reg.borrow_mut().LCR, 7); // clear LCR
        uart.write(THR, 0xaau8);

        for i in 0..12 {
            for j in 0..UART_DEFAULT_DIV * 16 {
                uart.step();
            }
        }
        assert!((uart.read::<u8>(offset::LSR).unwrap() & (1 << 5)) != 0);
        assert!((uart.read::<u8>(offset::LSR).unwrap() & (1 << 6)) != 0);
        assert!((uart.read::<u8>(offset::LSR).unwrap() & 0x01) != 0);
        assert!(uart.read::<u8>(offset::RBR).unwrap() == 0xaa);
        assert!((uart.read::<u8>(offset::LSR).unwrap() & 0x01) == 0);
    }
}
