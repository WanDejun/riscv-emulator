use crate::config::arch_config::WordType;

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
