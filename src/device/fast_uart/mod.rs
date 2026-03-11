//! TODO: this module is not fully implemented according to the spec.
//! Some features are missing, and some behavior may be incorrect due to limited test coverage.

pub mod virtual_io;

use std::{
    cell::RefCell,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8},
    },
    u8,
};

use crossbeam::channel::{self, Receiver, Sender};

use crate::{
    EMULATOR_CONFIG,
    config::arch_config::WordType,
    device::{
        DeviceTrait, MemError, MemMappedDeviceTrait,
        config::{UART_BASE, UART_DEFAULT_DIV, UART_IRQ, UART_SIZE},
        fast_uart::virtual_io::{SerialDestination, TerminalIO},
    },
    utils::{clear_bit, read_bit, set_bit},
};

const UART_DATA_LENGTH: u8 = 8;

pub struct UartIOChannel {
    pub(crate) input_tx: Sender<u8>,
    pub(crate) output_rx: Receiver<u8>,
    pub(crate) busy: Arc<AtomicBool>,
    /// Shared IER register value for interrupt condition checking from the polling thread.
    pub(crate) ier: Arc<AtomicU8>,
    /// Shared THRE event latch used by the ETBEI/THRE interrupt path.
    pub(crate) thre_pending: Arc<AtomicBool>,
    /// PLIC interrupt source ID for this UART.
    pub(crate) interrupt_id: u32,
}

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
    FCR:    u8,         //  | Any   | +0x2  | FIFO Control Register             |   WO
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

#[allow(non_snake_case)]
pub struct FastUart16550 {
    reg: Arc<RefCell<Uart16550Reg>>,
    reg_ptr: [*const u8; 8],
    reg_mut_ptr: [*mut u8; 8],
    reg_lcr_ptr: [*mut u8; 8],

    input_tx: Sender<u8>,
    input_rx: Receiver<u8>,
    output_tx: Sender<u8>,
    output_rx: Receiver<u8>,
    sync_lock: Arc<AtomicBool>,
    /// Shared IER value for the polling thread to check interrupt conditions.
    ier_shared: Arc<AtomicU8>,
    /// THRE event latch for simplified ETBEI behavior.
    /// Cleared when IIR reports THRE as the identified interrupt source.
    thre_pending: Arc<AtomicBool>,
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
        let ier_shared = Arc::new(AtomicU8::new(0));
        let thre_pending = Arc::new(AtomicBool::new(true)); // THR is initially empty.

        drop(reg_ref);
        Self {
            reg: reg.clone(),
            reg_ptr,
            reg_mut_ptr,
            reg_lcr_ptr,
            input_tx,
            input_rx,
            output_tx,
            output_rx,
            sync_lock,
            ier_shared,
            thre_pending,
        }
    }

    /// Compute a simplified IIR (Interrupt Identification Register) view based on current IER/LSR/FCR state.
    fn compute_iir(&mut self) -> u8 {
        let reg = self.reg.borrow();
        let ier = reg.IER;
        let lsr = reg.LSR;
        let fcr = reg.FCR;

        let mut iir: u8 = 0x01;

        // FIFO enabled status in IIR[7:6]
        if fcr & 0x01 != 0 {
            iir |= 0xC0;
        }

        // Check interrupt conditions in priority order.
        if ier & 0x04 != 0 && lsr & 0x1E != 0 {
            // Receiver Line Status (OE, PE, FE, BI)
            iir = (iir & 0xC0) | 0x06;
        } else if ier & 0x01 != 0 && lsr & 0x01 != 0 {
            // Received Data Available
            iir = (iir & 0xC0) | 0x04;
        } else if ier & 0x02 != 0 && self.thre_pending.load(std::sync::atomic::Ordering::Acquire) {
            // Transmitter Holding Register Empty (edge-triggered)
            // Reading IIR with THRE identified clears the pending condition.
            self.thre_pending
                .store(false, std::sync::atomic::Ordering::Release);
            iir = (iir & 0xC0) | 0x02;
        } else if ier & 0x08 != 0 {
            // Modem Status
            iir = (iir & 0xC0) | 0x00;
        }

        log::trace!(
            "[UART] compute_iir: IER={:#04x} LSR={:#04x} thre_pending={} => IIR={:#04x}",
            ier,
            lsr,
            self.thre_pending.load(std::sync::atomic::Ordering::Relaxed),
            iir
        );

        iir
    }

    fn read_impl<T>(&mut self, inner_addr: WordType) -> Result<T, MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        // check terminal input.
        if !read_bit(&mut self.reg.borrow_mut().LSR, 0) {
            // receive data ready.
            if let Ok(data) = self.input_rx.try_recv() {
                self.write_RBR(data)
            }
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
                } else if i == 2 {
                    // IIR: compute dynamically instead of reading stale value
                    data |= T::from(self.compute_iir() << (8 * (i - inner_addr)));
                } else {
                    data |= T::from(
                        unsafe { self.reg_ptr[i].read_volatile() } << (8 * (i - inner_addr)),
                    );
                }
            }
        }

        Ok(data)
    }

    fn write_impl<T>(&mut self, inner_addr: WordType, data: T) -> Result<(), MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        let inner_addr: usize = inner_addr as usize;
        let size = size_of::<T>();
        assert!(inner_addr as usize + size <= 8);
        let mut data: u64 = data.into();

        if (self.reg.borrow().LCR & (1 << 7)) == (1 << 7) {
            // LCR (Divisor Latch Access)
            for i in inner_addr..8.min(inner_addr + size) {
                unsafe { self.reg_lcr_ptr[i].write_volatile((data & (0xff)) as u8) }
                data >>= 8;
            }
        } else {
            // Normal
            for i in inner_addr..8.min(inner_addr + size) {
                if i == 0 {
                    // Writing to THR: send the byte immediately.
                    let byte = (data & 0xff) as u8;
                    log::trace!(
                        "[UART] THR write: {:#04x} '{}'",
                        byte,
                        if byte.is_ascii_graphic() || byte == b' ' {
                            byte as char
                        } else {
                            '.'
                        }
                    );
                    let _ = self.output_tx.send(byte);
                    // In a real 16550, writing THR clears LSR[5] (THRE) momentarily,
                    // then sets it again when the shift register accepts the byte.
                    // Since fast_uart sends instantly, we just re-arm the THRE event.
                    self.thre_pending
                        .store(true, std::sync::atomic::Ordering::Release);
                } else {
                    unsafe { self.reg_mut_ptr[i].write_volatile((data & (0xff)) as u8) };
                    if i == 1 {
                        let new_ier = (data & 0xff) as u8;
                        let old_ier = self
                            .ier_shared
                            .swap(new_ier, std::sync::atomic::Ordering::AcqRel);
                        // Re-arm THRE event when ETBEI (IER bit1) transitions 0->1.
                        if new_ier & 0x02 != 0 && old_ier & 0x02 == 0 {
                            self.thre_pending
                                .store(true, std::sync::atomic::Ordering::Release);
                            log::trace!(
                                "[UART] IER write: {:#04x} -> {:#04x}, ETBEI newly set, thre_pending=true",
                                old_ier,
                                new_ier
                            );
                        } else {
                            log::trace!("[UART] IER write: {:#04x} -> {:#04x}", old_ier, new_ier);
                        }
                    }
                }
                data >>= 8;
            }
        }

        Ok(())
    }

    pub(crate) fn get_io_channel(&self) -> UartIOChannel {
        UartIOChannel {
            input_tx: self.input_tx.clone(),
            output_rx: self.output_rx.clone(),
            busy: self.sync_lock.clone(),
            ier: self.ier_shared.clone(),
            thre_pending: self.thre_pending.clone(),
            interrupt_id: UART_IRQ,
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

impl DeviceTrait for FastUart16550 {
    dispatch_read_write! { read_impl, write_impl }

    fn sync(&mut self) {
        if EMULATOR_CONFIG.lock().unwrap().serial_destination == SerialDestination::Test {
            return;
        }
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

    fn get_poll_event(&mut self) -> Option<Box<dyn crate::device_poller::PollingEventTrait>> {
        if EMULATOR_CONFIG.lock().unwrap().serial_destination == SerialDestination::Stdio {
            Some(Box::new(TerminalIO::new(self.get_io_channel())))
        } else {
            None
        }
    }
}

impl MemMappedDeviceTrait for FastUart16550 {
    fn base() -> WordType {
        UART_BASE
    }
    fn size() -> WordType {
        UART_SIZE
    }
}

#[cfg(test)]
mod test {
    use crate::{EmulatorConfigurator, device::fast_uart::virtual_io::SimulationIO};

    use super::*;

    #[test]
    fn output_test() {
        EmulatorConfigurator::new().set_serial_destination(SerialDestination::Test); // set test mode
        let mut uart = FastUart16550::new();
        let uart_dest = SimulationIO::new(uart.get_io_channel());

        uart.write_impl(0, 'a' as u8).unwrap();

        let receive = uart_dest.receive_output_data();
        assert_eq!(receive.len(), 1);
        assert_eq!(receive[0], 'a' as u8);
    }

    #[test]
    fn input_test() {
        EmulatorConfigurator::new().set_serial_destination(SerialDestination::Test);
        let mut uart = FastUart16550::new();
        let uart_dest = SimulationIO::new(uart.get_io_channel());

        uart_dest.send_input_data(['a' as u8, 'b' as u8, 'c' as u8, 'd' as u8]);

        assert_eq!(uart.read_impl::<u8>(5).unwrap() & 1u8, 1);
        assert_eq!(uart.read_impl::<u8>(0).unwrap(), 'a' as u8);
        assert_eq!(uart.read_impl::<u8>(0).unwrap(), 'b' as u8);
        assert_eq!(uart.read_impl::<u8>(0).unwrap(), 'c' as u8);
        assert_eq!(uart.read_impl::<u8>(0).unwrap(), 'd' as u8);
        assert_eq!(uart.read_impl::<u8>(5).unwrap() & 1u8, 0);
    }
}
