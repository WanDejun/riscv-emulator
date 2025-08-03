use crate::config::arch_config::WordType;
pub mod uart;

pub trait DeviceTrait {
    fn write<T>(device: &mut T, addr: usize, data: WordType) where T: DeviceTrait;
    fn read<T>(device: &mut T, addr: usize, data: WordType) where T: DeviceTrait;
}