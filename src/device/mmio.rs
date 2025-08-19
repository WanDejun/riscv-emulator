use std::cmp::Ordering;

use crate::{
    config::arch_config::WordType,
    device::{
        DebugUart, DeviceTrait, Mem, MemError,
        config::{
            Device, MMIO_FREQ_DIV, POWER_MANAGER_ADDR, POWER_MANAGER_SIZE, UART_SIZE, UART1_ADDR,
        },
        power_manager::PowerManager,
        uart::Uart16550,
    },
    ram::Ram,
    ram_config,
    utils::{UnsignedInteger, check_align},
};

struct MemoryMapItem {
    pub start: WordType,
    pub size: WordType,
    // pub name: &'static str,
    pub device: Device,
}

impl PartialEq for MemoryMapItem {
    fn eq(&self, other: &Self) -> bool {
        self.start == other.start
    }
}
impl Eq for MemoryMapItem {}
impl PartialOrd for MemoryMapItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for MemoryMapItem {
    fn cmp(&self, other: &Self) -> Ordering {
        self.start.cmp(&other.start)
    }
}

impl MemoryMapItem {
    fn new(start: WordType, size: WordType, device: Device) -> Self {
        Self {
            start,
            size,
            device,
        }
    }
}

// struct MMIOLockGuard {
//     p: Rc<AtomicBool>,
// }

// impl MMIOLockGuard {
//     fn new(p: Rc<AtomicBool>) -> Self {
//         Self { p }
//     }
// }
// impl Drop for MMIOLockGuard {
//     fn drop(&mut self) {
//         self.p.store(false, std::sync::atomic::Ordering::Release);
//     }
// }

/// # mmio
/// ## Usage
/// make sure the address was aligned.
/// ```
/// let mut mmio = MemoryMapIO::new();
/// let a = mmio.read::<WordType>(ram_config::BASE_ADDR + 0x08);
/// let b = mmio.read::<u8>(UART1_ADDR + 0x00);
/// mmio.write::<u8>(UART1_ADDR + 0x06);
/// mmio.write::<u32>(ram_config::BASE_ADDR + 0x03); // ILLIGAL! unaligned accesses
/// ```
pub struct MemoryMapIO {
    // atomic_lock: Rc<AtomicBool>,
    dev_counter: usize,
    debug_uart: DebugUart,
    map: Vec<MemoryMapItem>,
}

impl MemoryMapIO {
    pub fn new() -> Self {
        Self::from_ram(Ram::new())
    }

    pub fn from_ram(ram: Ram) -> Self {
        let ram = Device::Ram(ram);

        let mut uart1 = Uart16550::new(0 as *const u8);
        let mut debug_uart = DebugUart::new(0 as *const u8);
        let power_manager = Device::PowerManager(PowerManager::new());

        // Uart
        uart1.change_rx_wiring(debug_uart.uart.get_tx_wiring());
        debug_uart.uart.change_rx_wiring(uart1.get_tx_wiring());

        #[cfg(not(test))]
        {
            use crate::device::cli_uart::spawn_io_thread;
            spawn_io_thread(debug_uart.input_tx.clone(), debug_uart.output_rx.clone());
        }

        let map = vec![
            MemoryMapItem::new(POWER_MANAGER_ADDR, POWER_MANAGER_SIZE, power_manager),
            MemoryMapItem::new(UART1_ADDR, UART_SIZE, Device::Uart16550(uart1)),
            MemoryMapItem::new(ram_config::BASE_ADDR, ram_config::SIZE as u64, ram),
        ];

        Self {
            // atomic_lock: Rc::new(AtomicBool::new(false)),
            dev_counter: 0,
            debug_uart,
            map,
        }
    }

    fn read_from_device<T>(&mut self, device_index: usize, p_addr: WordType) -> Result<T, MemError>
    where
        T: UnsignedInteger,
    {
        if !check_align::<T>(p_addr) {
            return Err(MemError::LoadMisaligned);
        }
        let start = self.map[device_index].start;
        if p_addr >= start + self.map[device_index].size {
            // out of range
            Err(MemError::LoadFault)
            // panic!(
            //     "read_from_device(index: {}, p_addr: {}): physical address overflow",
            //     device_index, p_addr
            // )
        } else {
            // in range
            let device = &mut self.map[device_index].device;
            device.read(p_addr - start)
        }
    }

    // write data to specific device.
    fn write_to_device<T>(
        &mut self,
        device_index: usize,
        p_addr: WordType,
        data: T,
    ) -> Result<(), MemError>
    where
        T: UnsignedInteger,
    {
        if !check_align::<T>(p_addr) {
            return Err(MemError::StoreMisaligned);
        }
        let st = self.map[device_index].start;
        if p_addr >= st + self.map[device_index].size {
            // out of range
            panic!(
                "write_to_device(index: {}, p_addr: {}, data: {}): physical address overflow",
                device_index, p_addr, data
            )
        } else {
            // in range
            let device = &mut self.map[device_index].device;
            device.write(p_addr - st, data)
        }
    }

    // fn lock(&mut self) -> MMIOLockGuard {
    //     loop {
    //         if self
    //             .atomic_lock
    //             .swap(true, std::sync::atomic::Ordering::Acquire)
    //             == false
    //         {
    //             break;
    //         }
    //     }

    //     MMIOLockGuard::new(self.atomic_lock.clone())
    // }
}

impl Mem for MemoryMapIO {
    fn read<T>(&mut self, p_addr: WordType) -> Result<T, MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        // let _guard = self.lock();
        match self.map.binary_search_by(|device| {
            if p_addr < device.start {
                Ordering::Greater
            } else if p_addr > device.start {
                Ordering::Less
            } else {
                Ordering::Equal
            }
        }) {
            Ok(i) => self.read_from_device(i, p_addr),
            Err(i) => {
                if i == 0 {
                    Err(MemError::LoadFault)
                    // panic!("physical address: {} is not mapped to the device", p_addr);
                } else {
                    self.read_from_device(i - 1, p_addr)
                }
            }
        }
    }

    fn write<T>(&mut self, p_addr: WordType, data: T) -> Result<(), MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        // let _guard = self.lock();
        match self.map.binary_search_by(|device| {
            if p_addr < device.start {
                Ordering::Greater
            } else if p_addr > device.start {
                Ordering::Less
            } else {
                Ordering::Equal
            }
        }) {
            Ok(i) => self.write_to_device(i, p_addr, data),
            Err(i) => {
                if i == 0 {
                    // panic!("physical address: {} is not mapped to the device", p_addr);
                    Err(MemError::StoreFault)
                } else {
                    self.write_to_device(i - 1, p_addr, data)
                }
            }
        }
    }
}

/// # MemoryMapIO
/// ## NOTE
/// If there was more than one hart/core for emulator. Only one hart/core is allowed to do `MemoryMapIO::step`.
impl DeviceTrait for MemoryMapIO {
    fn step(&mut self) {
        // let _guard = self.lock();
        if self.dev_counter == MMIO_FREQ_DIV {
            self.dev_counter = 0;

            for item in self.map.iter_mut() {
                item.device.step();
            }
            self.debug_uart.step();
        }

        self.dev_counter += 1;
    }
    fn sync(&mut self) {
        // let _guard = self.lock();
        for item in self.map.iter_mut() {
            item.device.sync();
        }
        self.debug_uart.sync();
    }
}

#[cfg(test)]
mod test {
    use crate::device::{config::UART_DEFAULT_DIV, peripheral_init};

    use super::*;

    #[test]
    fn mmio_mem_test() {
        let mut mmio = MemoryMapIO::new();
        for i in 0 as WordType..100 {
            mmio.write(ram_config::BASE_ADDR + i * (1 << size_of::<WordType>()), i)
                .unwrap();
        }

        for i in 0 as WordType..100 {
            assert_eq!(
                i,
                mmio.read(ram_config::BASE_ADDR + i * (1 << size_of::<WordType>()))
                    .unwrap()
            );
        }
    }

    #[test]
    fn mmio_stdout_test() {
        let _handles = peripheral_init();
        let mut mmio = MemoryMapIO::new();
        mmio.write(UART1_ADDR + 0x00, 'a' as u8).unwrap();
        for _ in 0..MMIO_FREQ_DIV * UART_DEFAULT_DIV * 16 * 20 {
            mmio.step();
        }
        assert_ne!((mmio.read::<u8>(UART1_ADDR + 5).unwrap() & 0x20), 0);
        assert_eq!((mmio.debug_uart.uart.read::<u8>(5).unwrap() & 0x01), 0);
        assert_eq!(mmio.debug_uart.receive(), Some('a' as u8));
    }

    #[test]
    fn mmio_stdio_test() {
        let _handles = peripheral_init();
        let mut mmio = MemoryMapIO::new();
        mmio.debug_uart.send('x' as u8);
        loop {
            mmio.step();
            if (mmio.read::<u8>(UART1_ADDR + 5).unwrap() & 0x01) != 0 {
                assert_eq!(mmio.read::<u8>(UART1_ADDR + 0).unwrap(), 'x' as u8);
                break;
            }
        }
    }
}
