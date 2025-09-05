pub(crate) trait VirtIODeviceTrait {
    fn set_queue_num(&mut self, num: u32);

    fn set_desc_low(&mut self, addr_low: u32);
    fn set_desc_high(&mut self, addr_high: u32);
    fn set_avail_low(&mut self, addr_low: u32);
    fn set_avail_high(&mut self, addr_high: u32);
    fn set_used_low(&mut self, addr_low: u32);
    fn set_used_high(&mut self, addr_high: u32);

    fn manage_one_request(&mut self) -> bool;
    fn notify(&mut self);
}
