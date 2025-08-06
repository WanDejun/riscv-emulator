use crate::config::arch_config::WordType;
pub mod uart;

pub trait DeviceTrait {
    fn write(device: &mut Self, addr: usize, data: WordType);
    fn read(device: &mut Self, addr: usize, data: WordType) -> WordType;
}
