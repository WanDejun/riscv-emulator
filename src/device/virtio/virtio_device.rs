use std::sync::atomic::AtomicU8;

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
}
