use std::{cell::RefCell, cmp::Ordering, rc::Rc};

use crate::{
    config::arch_config::WordType,
    device::{
        DeviceTrait, Mem,
        cli_uart::Cli,
        config::{Device, UART_SIZE, UART1_ADDR},
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
    pub device: Rc<RefCell<Device>>,
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
    fn new(start: WordType, size: WordType, device: Rc<RefCell<Device>>) -> Self {
        Self {
            start,
            size,
            device,
        }
    }
}

/// # mmio
/// ## Usage
/// make sure the address was aligned.
/// ```
/// let mut mmio = MemoryMapIO::new();
/// let a = mmio.read::<WordType>(mem_config::BASE_ADDR + 0x08);
/// let b = mmio.read::<u8>(UART1_ADDR + 0x00);
/// mmio.write::<u8>(UART1_ADDR + 0x06);
/// mmio.write::<u32>(mem_config::BASE_ADDR + 0x03); // ILLIGAL! unaligned accesses
/// ```
pub struct MemoryMapIO {
    cli: Box<Cli>,
    map: Vec<MemoryMapItem>,
}

impl MemoryMapIO {
    #[allow(unused)]
    pub fn new() -> Self {
        let mut cli = Box::new(Cli::new(0 as *const u8));

        let uart1 = Rc::new(RefCell::new(Device::Uart16550(Uart16550::new(
            cli.uart.get_tx_wiring(),
        ))));
        if let Device::Uart16550(uart1_inner) = &*uart1.borrow() {
            cli.uart.change_rx_wiring(uart1_inner.get_tx_wiring());
        }

        let ram = Rc::new(RefCell::new(Device::Ram(Ram::new())));

        let map = vec![
            MemoryMapItem::new(UART1_ADDR, UART_SIZE, uart1.clone()),
            MemoryMapItem::new(ram_config::BASE_ADDR, ram_config::SIZE as u64, ram.clone()),
        ];
        Self {
            cli,
            map,
        }
    }

    fn read_from_device<T>(&mut self, index: usize, p_addr: WordType) -> T
    where
        T: UnsignedInteger,
    {
        check_align::<T>(p_addr);
        let start = self.map[index].start;
        if p_addr >= start + self.map[index].size {
            // out of range
            panic!(
                "read_from_device(index: {}, p_addr: {}): physical address overflow",
                index, p_addr
            )
        } else {
            // in range
            self.map[index].device.borrow_mut().read(p_addr - start)
        }
    }

    // write data to specific device.
    fn write_to_device<T>(&mut self, device_index: usize, p_addr: WordType, data: T)
    where
        T: UnsignedInteger,
    {
        check_align::<T>(p_addr);
        let st = self.map[device_index].start;
        if p_addr >= st + self.map[device_index].size {
            // out of range
            panic!(
                "write_to_device(index: {}, p_addr: {}, data: {}): physical address overflow",
                device_index, p_addr, data
            )
        } else {
            // in range
            self.map[device_index]
                .device
                .borrow_mut()
                .write(p_addr - st, data)
        }
    }
}

impl Mem for MemoryMapIO {
    fn read<T>(&mut self, p_addr: WordType) -> T
    where
        T: crate::utils::UnsignedInteger,
    {
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
                    panic!("physical address: {} is not mapped to the device", p_addr);
                } else {
                    self.read_from_device(i - 1, p_addr)
                }
            }
        }
    }

    fn write<T>(&mut self, p_addr: WordType, data: T)
    where
        T: crate::utils::UnsignedInteger,
    {
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
                    panic!("physical address: {} is not mapped to the device", p_addr);
                } else {
                    self.write_to_device(i - 1, p_addr, data);
                }
            }
        }
    }
}

impl DeviceTrait for MemoryMapIO {
    fn one_shot(&mut self) {
        for item in self.map.iter() {
            item.device.borrow_mut().one_shot();
        }
        self.cli.one_shot();
    }
}

#[cfg(test)]
mod test {
    use crate::device::config::UART_DEFAULT_DIV;

    use super::*;

    #[test]
    fn mmio_mem_test() {
        let mut mmio = MemoryMapIO::new();
        for i in 0 as WordType..100 {
            mmio.write(ram_config::BASE_ADDR + i * (1 << size_of::<WordType>()), i);
        }

        for i in 0 as WordType..100 {
            assert_eq!(
                i,
                mmio.read(ram_config::BASE_ADDR + i * (1 << size_of::<WordType>()))
            );
        }
    }

    #[test]
    fn mmio_stdout_test() {
        let mut mmio = MemoryMapIO::new();
        mmio.write(UART1_ADDR + 0x00, 'a' as u8);
        for _ in 0..UART_DEFAULT_DIV * 16 * 20 {
            mmio.one_shot();
        }
        assert!((mmio.read::<u8>(UART1_ADDR + 5) & 0x20) != 0);
        assert!((mmio.cli.uart.read::<u8>(5) & 0x01) == 0);
    }

    #[ignore = "debug"]
    #[test]
    /// just for debug, not an test.
    fn mmio_stdin_test() {
        let mut mmio = MemoryMapIO::new();
        loop {
            mmio.one_shot();
            if (mmio.read::<u8>(UART1_ADDR + 5) & 0x01) != 0 {
                print!("{}", mmio.read::<u8>(UART1_ADDR + 0));
                break;
            }
        }
    }
}
