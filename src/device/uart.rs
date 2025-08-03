use std::{cell::RefCell, rc::Rc, u8};

use log::error;

#[allow(unused)]
mod offset {
    const RBR: usize = 0x00;
    const THR: usize = 0x00;
    const IER: usize = 0x04;
    const IIR: usize = 0x08;
    const FCR: usize = 0x08;
    const LCR: usize = 0x0C;
    const MCR: usize = 0x10;
    const LSR: usize = 0x14;
    const MSR: usize = 0x18;
    const SCR: usize = 0x1C;
    const DLL: usize = 0x00;
    const DLM: usize = 0x04;
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
            LCR: 0,
            MCR: 0,
            LSR: 0,
            MSR: 0,
            SCR: 0,
            DLL: u8::MAX,
            DLM: u8::MAX,
        }
    }

    fn get_divisor(&self) -> u16 {
        (self.DLL as u16) + ((self.DLM as u16) << 8)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Uart16550Status {
    IDLE,
    START,
    DATA(u8),
    STOP,
    END,
}

struct Uart16550TX {}
impl Uart16550TX {
    fn new(uart_reg: &Rc<RefCell<Uart16550Reg>>, rx_wiring: *mut u8) -> Self {
        Self {}
    }
}

struct Uart16550RX {
    uart_reg: Rc<RefCell<Uart16550Reg>>,
    status: Uart16550Status,
    div_counter: u16, // count frequency. Take one sample in DLL + (DLM << 8) clocks;
    sample_data: u8,  // Increasing when get high bit.
    rx_wiring: *const u8,

    sample_count: u8, // 16 times sampling for a bit
    bit_counter: u8,
}

impl Uart16550RX {
    fn new(uart_reg: &Rc<RefCell<Uart16550Reg>>, rx_wiring: *const u8) -> Self {
        Self {
            uart_reg: uart_reg.clone(),
            status: Uart16550Status::IDLE,
            div_counter: 0,
            sample_data: 0,
            sample_count: 0,
            bit_counter: 0,
            rx_wiring,
        }
    }

    fn write_data2reg(&self, data: u8) {
        self.uart_reg.borrow_mut().RBR = data;
        self.uart_reg.borrow_mut().LSR |= 0x01; // set receive data ready.
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
                    self.bit_counter = 0;
                    self.status = Uart16550Status::DATA(0);
                }
            }
            Uart16550Status::DATA(cur) => {
                let mut nxt = cur;
                if bit_data {
                    nxt |= 1 << self.bit_counter;
                }
                self.bit_counter += 1;

                if self.bit_counter == 8 {
                    self.write_data2reg(nxt);
                    self.status = Uart16550Status::IDLE;
                } else {
                    self.status = Uart16550Status::DATA(nxt);
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

    fn one_shot(&mut self) {
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
}

#[allow(non_snake_case)]
pub struct Uart16550 {
    reg: Rc<RefCell<Uart16550Reg>>,
    reg_ptr: [*const u8; 8],
    reg_mut_ptr: [*mut u8; 8],
    reg_lcr_ptr: [*mut u8; 8],

    rx: Uart16550RX,

    tx_data_vaild: bool,
    tx: Uart16550TX,
}

impl Uart16550 {
    pub fn new(rx_wiring: *const u8, tx_wiring: *mut u8) -> Self {
        let reg = Rc::new(RefCell::new(Uart16550Reg::new()));
        let mut reg_ref = reg.borrow_mut();
        let reg_ptr = [
            (&reg_ref.DLL) as *const u8,
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
            tx_data_vaild: false,
            tx: Uart16550TX::new(&reg, tx_wiring),
        }
    }

    pub fn one_shot(&mut self) {
        self.rx.one_shot();
    }
}

#[cfg(test)]
mod test {
    use std::{cell::RefCell, rc::Rc};

    use log::error;

    use crate::{
        device::uart::{Uart16550RX, Uart16550Reg},
        *,
    };

    #[test]
    fn rx_test() {
        let uart_reg = Rc::new(RefCell::new(Uart16550Reg::new()));
        let mut rx_wiring: u8 = 1;
        let mut rx = Uart16550RX::new(&uart_reg, (&rx_wiring) as *const u8);

        uart_reg.borrow_mut().DLL = 0xe8;
        uart_reg.borrow_mut().DLM = 0x03;
        uart_reg.borrow_mut().LCR = 0x03;

        for _ in 0..10000 {
            rx.one_shot();
        }

        assert!(uart_reg.borrow_mut().DLL == 0xe8);
        assert!(uart_reg.borrow_mut().DLM == 0x03);
        assert!(uart_reg.borrow_mut().LSR == 0x00);

        let data: [u8; 14] = [1, 1, 0x00, 1, 0, 1, 0, 1, 0, 1, 0, 1, 1, 1]; // 0x55
        for data_bit in data {
            for _ in 0..1000 * 16 {
                rx_wiring = data_bit;
                rx.one_shot();
            }
        }
        assert!(uart_reg.borrow_mut().LSR | 0x01 == 0x01);
        assert!(uart_reg.borrow_mut().RBR == 0x55);
    }
}
