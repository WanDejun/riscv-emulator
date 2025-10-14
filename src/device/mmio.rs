use std::{
    cell::{RefCell, UnsafeCell},
    cmp::Ordering,
    rc::Rc,
};

use crate::{
    config::arch_config::WordType,
    device::{DeviceTrait, Mem, MemError, config::Device},
    ram::Ram,
    ram_config,
    utils::{UnsignedInteger, check_align},
};

pub struct MemoryMapItem {
    pub(crate) start: WordType,
    pub(crate) size: WordType,
    pub(crate) device: Rc<RefCell<Device>>,
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
    pub(crate) fn new(start: WordType, size: WordType, device: Rc<RefCell<Device>>) -> Self {
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
/// let a = mmio.read::<WordType>(ram_config::BASE_ADDR + 0x08);
/// let b = mmio.read::<u8>(UART1_ADDR + 0x00);
/// mmio.write::<u8>(UART1_ADDR + 0x06);
/// mmio.write::<u32>(ram_config::BASE_ADDR + 0x03); // ILLIGAL! unaligned accesses
/// ```
pub struct MemoryMapIO {
    map: Vec<MemoryMapItem>,
    ram: Rc<UnsafeCell<Ram>>,
}

impl MemoryMapIO {
    pub fn from_mmio_items(ram: Rc<UnsafeCell<Ram>>, mut map: Vec<MemoryMapItem>) -> Self {
        map.sort();
        Self { map, ram }
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
        } else {
            // in range
            let device = &mut self.map[device_index].device;
            device.borrow_mut().read(p_addr - start)
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
            Err(MemError::StoreFault)
        } else {
            // in range
            let device = &mut self.map[device_index].device;
            device.borrow_mut().write(p_addr - st, data)
        }
    }
}

impl Mem for MemoryMapIO {
    fn read<T>(&mut self, p_addr: WordType) -> Result<T, MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        if p_addr >= ram_config::BASE_ADDR {
            return unsafe {
                self.ram
                    .as_mut_unchecked()
                    .read(p_addr - ram_config::BASE_ADDR)
            };
        }

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
        if p_addr >= ram_config::BASE_ADDR {
            return unsafe {
                self.ram
                    .as_mut_unchecked()
                    .write(p_addr - ram_config::BASE_ADDR, data)
            };
        }
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

impl DeviceTrait for MemoryMapIO {
    fn sync(&mut self) {
        // let _guard = self.lock();
        for item in self.map.iter_mut() {
            item.device.borrow_mut().sync();
        }
    }
}

#[cfg(test)]
mod test {
    use crate::device::{
        config::{POWER_MANAGER_BASE, POWER_MANAGER_SIZE, UART_BASE, UART_SIZE},
        fast_uart::{FastUart16550, virtual_io::SimulationIO},
        peripheral_init,
        power_manager::PowerManager,
    };

    use super::*;

    #[test]
    fn mmio_mem_test() {
        let ram = Rc::new(UnsafeCell::new(Ram::new()));
        let uart1 = FastUart16550::new();
        let power_manager = Device::PowerManager(PowerManager::new());
        let table = vec![
            MemoryMapItem::new(
                POWER_MANAGER_BASE,
                POWER_MANAGER_SIZE,
                Rc::new(RefCell::new(power_manager)),
            ),
            MemoryMapItem::new(
                UART_BASE,
                UART_SIZE,
                Rc::new(RefCell::new(Device::FastUart16550(uart1))),
            ),
        ];

        let mut mmio = MemoryMapIO::from_mmio_items(ram, table);
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
        let ram = Rc::new(UnsafeCell::new(Ram::new()));
        let uart1 = FastUart16550::new();
        let io: SimulationIO = SimulationIO::new(uart1.get_io_channel());
        let power_manager = Device::PowerManager(PowerManager::new());
        let table = vec![
            MemoryMapItem::new(
                POWER_MANAGER_BASE,
                POWER_MANAGER_SIZE,
                Rc::new(RefCell::new(power_manager)),
            ),
            MemoryMapItem::new(
                UART_BASE,
                UART_SIZE,
                Rc::new(RefCell::new(Device::FastUart16550(uart1))),
            ),
        ];

        let mut mmio = MemoryMapIO::from_mmio_items(ram, table);
        let _handles = peripheral_init();

        mmio.write(UART_BASE + 0x00, 'a' as u8).unwrap();
        assert_ne!((mmio.read::<u8>(UART_BASE + 5).unwrap() & 0x20), 0);
        let data = io.receive_output_data();
        assert_eq!(data.len(), 1);
        assert_eq!(data[0], 'a' as u8);
    }
}
