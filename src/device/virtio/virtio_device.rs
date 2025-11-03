use std::sync::{
    Mutex,
    atomic::{AtomicU8, AtomicU16},
};

use lazy_static::lazy_static;

pub(crate) trait VirtIODeviceTrait {
    fn get_device_id(&self) -> u16;
    fn status(&mut self) -> &mut u8;
    fn get_generation(&self) -> u32;

    fn isr(&mut self) -> &mut AtomicU8;
    fn update_irq(&mut self);

    fn get_host_feature(&self) -> u64;
    fn set_feature(&mut self, feature: u64);

    fn set_queue_num(&mut self, num: u32);
    fn queue_ready(&self) -> bool;
    fn queue_select(&self, idx: u32);
    fn get_num_of_queue(&self) -> u32; // device may have queue more than one.

    fn set_desc(&mut self, addr: u64);
    fn set_avail(&mut self, addr: u64);
    fn set_used(&mut self, addr: u64);

    fn manage_one_request(&mut self) -> bool;
    fn notify(&mut self, queue_idx: u32);

    fn read_config(&mut self, idx: u64) -> u32;
    fn write_config(&mut self, idx: u64, data: u32);

    fn get_poll_enent(&mut self) -> Option<crate::async_poller::PollingEvent> {
        None
    }
}

pub(super) struct DeviceIDAllocator(AtomicU16);

impl DeviceIDAllocator {
    pub(super) fn new() -> Self {
        Self(AtomicU16::new(0))
    }
    pub(super) fn alloc(&mut self) -> u16 {
        self.0.fetch_add(1, std::sync::atomic::Ordering::AcqRel)
    }
}

lazy_static! {
    pub(super) static ref DEVICE_ID_ALLOCTOR: Mutex<DeviceIDAllocator> =
        Mutex::new(DeviceIDAllocator::new());
}
