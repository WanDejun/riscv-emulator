use super::{DeviceTrait, Mem};
use crate::{
    config::arch_config::WordType,
    device::{
        MemError, aclint::Clint, fast_uart::FastUart16550, plic::PLIC, power_manager::PowerManager,
        virtio::virtio_mmio::VirtIOMMIO,
    },
    utils::UnsignedInteger,
};

#[cfg(feature = "test-device")]
use crate::device::test_device::TestDevice;

// TODO add size() fn to DeviceTrait
pub const POWER_MANAGER_NAME: &'static str = "virt-power";
pub const POWER_MANAGER_BASE: WordType = 0x10_0000;
pub const POWER_MANAGER_SIZE: WordType = 2;

#[cfg(feature = "test-device")]
pub const TEST_DEVICE_BASE: WordType = 0x10_1000;
#[cfg(feature = "test-device")]
pub const TEST_DEVICE_SIZE: WordType = 0x0f;

pub const CLINT_NAME: &'static str = "clint";
pub const CLINT_BASE: WordType = 0x200_0000;
pub const CLINT_SIZE: WordType = 0x10000;

pub const PLIC_NAME: &'static str = "plic";
pub const PLIC_BASE: WordType = 0xc00_0000;
pub const PLIC_SIZE: WordType = 0x400_0000;

pub const UART_DEFAULT_DIV: usize = 1;
pub const UART_NAME: &'static str = "uart";
pub const UART_BASE: WordType = 0x1000_0000;
pub const UART_SIZE: WordType = 8;

pub const VIRTIO_MMIO_NAME: &'static str = "virtio-mmio-device";
pub const VIRTIO_MMIO_BASE: WordType = 0x1000_1000;
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
            fn get_poll_enent(&mut self) -> Option<crate::async_poller::PollingEvent> {
                match self {
                    $( Device::$name(dev) => dev.get_poll_enent(), )*
                }
            }
        }
    };
}

#[cfg(not(feature = "test-device"))]
make_device_enum!(FastUart16550, PowerManager, Clint, VirtIOMMIO, PLIC);

#[cfg(feature = "test-device")]
make_device_enum!(
    FastUart16550,
    PowerManager,
    Clint,
    VirtIOMMIO,
    PLIC,
    TestDevice
);
