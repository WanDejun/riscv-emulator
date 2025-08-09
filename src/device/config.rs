use super::{DeviceTrait, Mem};
use crate::{
    config::arch_config::WordType, device::uart::Uart16550, ram::Ram, utils::UnsignedInteger,
};

pub const UART_DEFAULT_DIV: usize = 100;
pub const UART1_ADDR: WordType = 0x10000000;
pub const UART_SIZE: WordType = 8;

macro_rules! make_device_enum {
    ( $($name:ident),* $(,)? ) => {
        // #[derive(Debug)]
        pub enum Device {
            $( $name($name), )*
        }

        impl Mem for Device {
            fn read<T>(&mut self, addr: WordType) -> T
            where
                T: UnsignedInteger,
            {
                match self {
                    $( Device::$name(dev) => dev.read(addr), )*
                }
            }

            fn write<T>(&mut self, addr: WordType, data: T)
            where
                T: UnsignedInteger,
            {
                match self {
                    $( Device::$name(dev) => dev.write(addr, data), )*
                }
            }
        }

        impl DeviceTrait for Device {
            fn one_shot(&mut self) {
                match self {
                    $( Device::$name(dev) => dev.one_shot(), )*
                }
            }
        }
    };
}
make_device_enum!(Ram, Uart16550);
