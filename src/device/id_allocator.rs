use crate::{config::arch_config::WordType, device::MemMappedDeviceTrait};

pub(crate) struct MemMapInfo {
    pub(crate) name: String,
    pub(crate) base: WordType,
    pub(crate) size: WordType,
}

pub(crate) struct IdAllocator {
    id: WordType,
    device_name: String,
    mem_base: WordType,
    mem_size: WordType,
}

impl IdAllocator {
    pub(crate) fn new<T>(start_id: WordType, device_name: String) -> Self
    where
        T: MemMappedDeviceTrait,
    {
        Self {
            id: start_id,
            device_name,
            mem_base: T::base(),
            mem_size: T::size(),
        }
    }

    pub(crate) fn get(&mut self) -> MemMapInfo {
        let name = self.id.to_string() + &self.device_name;
        let mem = self.mem_base + self.id * self.mem_size;
        self.id += 1;
        MemMapInfo {
            name,
            base: mem,
            size: self.mem_size,
        }
    }
}
