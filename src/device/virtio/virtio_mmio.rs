use std::{cell::UnsafeCell, rc::Rc};

use bitflags::bitflags;
use log::error;
use num_enum::TryFromPrimitive;

use crate::{
    device::virtio::{config::*, virtio_blk::VirtIOBlkDevice},
    utils::check_align,
};

#[repr(u32)]
#[allow(unused)]
enum VirtIODeviceID {
    Network,
    Block,
    Console,
    Entropy,
    Balloon,
    SCSIHost,
    GPU,
    Input,
    Crypto,
    Socket,
    FileSystem,
    RPMB,
    IOMMU,
    Sound,
    Memory,
    I2CAdapter,
    SCMI,
    GPIO,
    PMEM,
}

#[repr(u64)]
#[derive(TryFromPrimitive, Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum VirtIO_MMIO_Offset {
    MagicValue = 0x000,
    Version = 0x004,
    DeviceId = 0x008,
    VendorId = 0x00C,
    DeviceFeatures = 0x010,
    DeviceFeaturesSelect = 0x014,
    DriverFeatures = 0x020,
    DriverFeaturesSelect = 0x024,
    QueueSelect = 0x030,
    QueueNumMax = 0x034,
    QueueNum = 0x038,
    QueueReady = 0x044,
    QueueNotify = 0x050,
    InterruptStatus = 0x060,
    InterruptAck = 0x064,
    Status = 0x070,
    QueueDescLow = 0x080,
    QueueDescHigh = 0x084,
    QueueAvailLow = 0x090,  // Driver ring
    QueueAvailHigh = 0x094, // Driver ring
    QueueUsedLow = 0x0A0,   // Device ring
    QueueUsedHigh = 0x0A4,  // Device ring
    SharedMemSelect = 0x0AC,
    SharedMemLenLow = 0x0B0,
    SharedMemLenHigh = 0x0B4,
    SharedMemBaseLow = 0x0B8,
    SharedMemBaseHigh = 0x0BC,
    // QueueReset = 0x0C0,
    ConfigGeneration = 0x0FC,
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct VirtIODeviceStatus: u32 {
        /// Device acknowledges that it has seen the device and knows what it is.
        const ACKNOWLEDGE = 1 << 0;
        /// Driver has found a usable device.
        const DRIVER = 1 << 1;
        /// Driver has set up the device.
        const DRIVER_OK = 1 << 2;
        /// Driver has failed to set up the device or device has encountered an error.
        const FAILED = 1 << 7;
        /// Device needs to be reset.
        const FEATURES_OK = 1 << 3;
    }
}

struct VirtIOMMIO {
    device: Rc<UnsafeCell<VirtIOBlkDevice>>,
    host_features_sel: u32,
}

impl VirtIOMMIO {
    pub fn new() -> Self {
        Self {
            device: Rc::new(UnsafeCell::new(VirtIOBlkDevice::new(
                "VirtIO Block Device".to_string(),
                VirtIODeviceID::Block as u16,
            ))),
            host_features_sel: 0,
        }
    }

    pub fn read(&self, offset: u64) -> u32 {
        let vdev = unsafe { self.device.as_mut_unchecked() };

        if check_align::<u32>(offset) {
            // will be checked in mmio.
            unreachable!()
        }

        let offset_type = VirtIO_MMIO_Offset::try_from(offset);
        if offset_type.is_err() {
            error!("VirtIO: read of unimplemented register: {:#x}", offset);
            return 0;
        }

        let ret = match unsafe { offset_type.unwrap_unchecked() } {
            VirtIO_MMIO_Offset::MagicValue => VIRT_MAGIC,
            VirtIO_MMIO_Offset::Version => VIRT_VERSION,
            VirtIO_MMIO_Offset::DeviceId => vdev.device_id as u32,
            VirtIO_MMIO_Offset::VendorId => VIRT_VENDOR,
            VirtIO_MMIO_Offset::DeviceFeatures => {
                // TODO!
                (vdev.host_feature >> self.host_features_sel) as u32
            }
            VirtIO_MMIO_Offset::QueueNumMax => VIRTQUEUE_MAX_SIZE,
            // VirtIO_MMIO_Offset::QueuePFN => ,
            VirtIO_MMIO_Offset::QueueReady => {
                // TODO! 
                0
            }
            VirtIO_MMIO_Offset::InterruptStatus => {
                vdev.isr.load(std::sync::atomic::Ordering::Relaxed) as u32
            }
            VirtIO_MMIO_Offset::Status => vdev.status as u32,
            VirtIO_MMIO_Offset::ConfigGeneration => vdev.generation,
            VirtIO_MMIO_Offset::SharedMemLenLow | VirtIO_MMIO_Offset::SharedMemLenHigh => u32::MAX,
            VirtIO_MMIO_Offset::DeviceFeaturesSelect
            | VirtIO_MMIO_Offset::DriverFeatures
            | VirtIO_MMIO_Offset::DriverFeaturesSelect
            // | VirtIO_MMIO_Offset::GuestPageSize
            | VirtIO_MMIO_Offset::QueueSelect
            | VirtIO_MMIO_Offset::QueueNum
            // | VirtIO_MMIO_Offset::QueueAlign
            | VirtIO_MMIO_Offset::QueueNotify
            | VirtIO_MMIO_Offset::InterruptAck
            | VirtIO_MMIO_Offset::QueueDescLow
            | VirtIO_MMIO_Offset::QueueDescHigh
            | VirtIO_MMIO_Offset::QueueAvailLow
            | VirtIO_MMIO_Offset::QueueAvailHigh
            | VirtIO_MMIO_Offset::QueueUsedLow
            | VirtIO_MMIO_Offset::QueueUsedHigh
             => {
                error!("VirtIO: read of write-only register: {:#x}", offset);
                unreachable!()
            }
            // VirtIO_MMIO_Offset::QueueReset | 
            VirtIO_MMIO_Offset::SharedMemBaseHigh | VirtIO_MMIO_Offset::SharedMemBaseLow | VirtIO_MMIO_Offset::SharedMemSelect => {
                error!("VirtIO: read of unimplemented register: {:#x}", offset);
                0
            }
        };

        ret
    }
}
