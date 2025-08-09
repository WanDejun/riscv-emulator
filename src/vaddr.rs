use crate::device::{DeviceTrait, Mem, mmio::MemoryMapIO};

pub struct VirtAddrManager {
    mmio: MemoryMapIO,
}

impl Mem for VirtAddrManager {
    fn read<T>(&mut self, addr: crate::config::arch_config::WordType) -> T
    where
        T: crate::utils::UnsignedInteger,
    {
        self.mmio.read(addr)
    }

    fn write<T>(&mut self, addr: crate::config::arch_config::WordType, data: T)
    where
        T: crate::utils::UnsignedInteger,
    {
        self.mmio.write(addr, data);
    }
}

impl DeviceTrait for VirtAddrManager {
    fn one_shot(&mut self) {
        self.mmio.one_shot();
    }
}
