use std::{
    cmp::Ordering,
    sync::{Arc, Mutex},
};

use crate::{
    config::arch_config::WordType,
    device::{
        DEBUG_UART, DeviceTrait, Mem, UART1,
        config::{Device, UART_SIZE, UART1_ADDR},
    },
    ram::Ram,
    ram_config,
    utils::{UnsignedInteger, check_align},
};

struct MemoryMapItem {
    pub start: WordType,
    pub size: WordType,
    // pub name: &'static str,
    pub device: Arc<Mutex<Device>>,
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
    fn new(start: WordType, size: WordType, device: Arc<Mutex<Device>>) -> Self {
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
    map: Vec<MemoryMapItem>,
}

impl MemoryMapIO {
    pub fn new() -> Self {
        Self::from_ram(Ram::new())
    }

    pub fn from_ram(ram: Ram) -> Self {
        let ram = Arc::new(Mutex::new(Device::Ram(ram)));

        let map = vec![
            MemoryMapItem::new(UART1_ADDR, UART_SIZE, UART1.clone()),
            MemoryMapItem::new(ram_config::BASE_ADDR, ram_config::SIZE as u64, ram.clone()),
        ];
        Self { map }
    }

    fn read_from_device<T>(&mut self, device_index: usize, p_addr: WordType) -> T
    where
        T: UnsignedInteger,
    {
        check_align::<T>(p_addr);
        let start = self.map[device_index].start;
        if p_addr >= start + self.map[device_index].size {
            // out of range
            panic!(
                "read_from_device(index: {}, p_addr: {}): physical address overflow",
                device_index, p_addr
            )
        } else {
            // in range
            let mut device = self.map[device_index].device.lock().unwrap();
            device.read(p_addr - start)
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
            let mut device = self.map[device_index].device.lock().unwrap();
            device.write(p_addr - st, data)
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
            item.device.lock().unwrap().one_shot();
        }
        DEBUG_UART.lock().unwrap().one_shot();
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
        let _handles = peripheral_init();
        let mut mmio = MemoryMapIO::new();
        mmio.write(UART1_ADDR + 0x00, 'a' as u8);
        for _ in 0..UART_DEFAULT_DIV * 16 * 20 {
            mmio.one_shot();
        }
        assert_ne!((mmio.read::<u8>(UART1_ADDR + 5) & 0x20), 0);
        assert_eq!((DEBUG_UART.lock().unwrap().uart.read::<u8>(5) & 0x01), 0);
        assert_eq!(DEBUG_UART.lock().unwrap().receive(), Some('a' as u8));
    }

    #[test]
    /// just for debug, not an test.
    fn mmio_stdio_test() {
        let _handles = peripheral_init();
        let mut mmio = MemoryMapIO::new();
        DEBUG_UART.lock().unwrap().send('x' as u8);
        loop {
            mmio.one_shot();
            if (mmio.read::<u8>(UART1_ADDR + 5) & 0x01) != 0 {
                assert_eq!(mmio.read::<u8>(UART1_ADDR + 0), 'x' as u8);
                break;
            }
        }
    }
}
