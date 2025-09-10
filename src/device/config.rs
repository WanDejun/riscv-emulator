use super::{DeviceTrait, Mem};
use crate::{
    config::arch_config::WordType,
    device::{
        MemError, aclint::Clint, fast_uart::FastUart16550, power_manager::PowerManager,
        virtio::virtio_mmio::VirtIOMMIO,
    },
    utils::UnsignedInteger,
};

// TODO add size() fn to DeviceTrait
pub const UART_DEFAULT_DIV: usize = 1;
pub const UART_SIZE: WordType = 8;
pub const UART1_ADDR: WordType = 0x10000000;

pub const POWER_MANAGER_SIZE: WordType = 2;
pub const POWER_MANAGER_ADDR: WordType = 0x100000;

pub const VIRTIO_MMIO_BASE: WordType = 0x10001000;
pub const VIRTIO_MMIO_SIZE: WordType = 0x1000;

// pub const MMIO_FREQ_DIV: usize = 32;

macro_rules! make_device_enum {
    ( $($name:ident),* $(,)? ) => {
        // #[derive(Debug)]
        pub(crate) enum Device {
            $( $name($name), )*
        }

        impl Mem for Device {
            fn read<T>(&mut self, addr: WordType) -> Result<T, MemError>
            where
                T: UnsignedInteger,
            {
                match self {
                    $( Device::$name(dev) => dev.read(addr), )*
                }
            }

            fn write<T>(&mut self, addr: WordType, data: T) -> Result<(), MemError>
            where
                T: UnsignedInteger,
            {
                match self {
                    $( Device::$name(dev) => dev.write(addr, data), )*
                }
            }
        }

        impl DeviceTrait for Device {
            fn sync(&mut self) {
                match self {
                    $( Device::$name(dev) => dev.sync(), )*
                }
            }
        }
    };
}
make_device_enum!(FastUart16550, PowerManager, Clint, VirtIOMMIO);
