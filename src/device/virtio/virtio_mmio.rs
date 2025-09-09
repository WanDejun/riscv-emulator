use std::{cell::UnsafeCell, rc::Rc};

use bitflags::bitflags;
use log::error;
use num_enum::TryFromPrimitive;

use crate::{
    device::virtio::{config::*, virtio_device::VirtIODeviceTrait},
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
    pub struct VirtIODeviceStatus: u8 {
        /// Device acknowledges that it has seen the device and knows what it is.
        const ACKNOWLEDGE = 1 << 0;
        /// Driver has found a usable device.
        const DRIVER = 1 << 1;
        /// Driver has set up the device.
        const DRIVER_OK = 1 << 2;
        /// Driver has failed to set up the device or device has encountered an error.
        const FAILED = 1 << 7;
        /// Indicates that the driver is set up and ready to drive the device
        const FEATURES_OK = 1 << 3;
        /// Device needs to be reset.
        const DEVICE_NEEDS_RESET = 1 << 6;
    }
}

#[derive(Clone, Copy)]
struct VirtIOMMIOQueueStatus {
    desc: u64,
    avail: u64,
    used: u64,
    enable: bool,
}
impl Default for VirtIOMMIOQueueStatus {
    fn default() -> Self {
        Self {
            desc: 0,
            avail: 0,
            used: 0,
            enable: false,
        }
    }
}

struct VirtIOMMIO {
    device: Rc<UnsafeCell<dyn VirtIODeviceTrait>>,
    host_features_sel: u32,
    host_features: u64,
    guest_features_sel: u32,
    guest_features: u64,

    queues: [VirtIOMMIOQueueStatus; 8],
    queue_select: u64,
}

impl VirtIOMMIO {
    pub fn new(device: Rc<UnsafeCell<dyn VirtIODeviceTrait>>) -> Self {
        Self {
            device,
            host_features_sel: 0,
            host_features: 0,
            guest_features_sel: 0,
            guest_features: 0,

            queues: [VirtIOMMIOQueueStatus::default(); 8],
            queue_select: 0,
        }
    }

    pub fn read(&self, offset: u64) -> u32 {
        let vdev = unsafe { self.device.as_mut_unchecked() };

        if !check_align::<u32>(offset) {
            // will be checked in mmio.
            unreachable!()
        }

        let offset_type = VirtIO_MMIO_Offset::try_from(offset);
        match offset_type {
            Err(error) => {
                error!(
                    "VirtIO: read of unimplemented register: {:#x}, {}",
                    offset, error
                );
                0
            }
            Ok(offset_type) => {
                let ret = match  offset_type {
                    VirtIO_MMIO_Offset::MagicValue => VIRT_MAGIC,
                    VirtIO_MMIO_Offset::Version => VIRT_VERSION,
                    VirtIO_MMIO_Offset::DeviceId => vdev.get_device_id() as u32,
                    VirtIO_MMIO_Offset::VendorId => VIRT_VENDOR,
                    VirtIO_MMIO_Offset::DeviceFeatures => {
                        // TODO!
                        (vdev.get_host_feature() >> self.host_features_sel) as u32
                    }
                    VirtIO_MMIO_Offset::QueueNumMax => VIRTQUEUE_MAX_SIZE,
                    // VirtIO_MMIO_Offset::QueuePFN => 0 as u32, // legacy
                    VirtIO_MMIO_Offset::QueueReady => {
                        vdev.queue_ready() as u32
                    }
                    VirtIO_MMIO_Offset::InterruptStatus => {
                        vdev.isr().load(std::sync::atomic::Ordering::Relaxed) as u32
                    }
                    VirtIO_MMIO_Offset::Status => *vdev.status() as u32,
                    VirtIO_MMIO_Offset::ConfigGeneration => vdev.get_generation(),
                    VirtIO_MMIO_Offset::SharedMemLenLow | VirtIO_MMIO_Offset::SharedMemLenHigh => u32::MAX,
                    VirtIO_MMIO_Offset::DeviceFeaturesSelect
                    | VirtIO_MMIO_Offset::DriverFeatures
                    | VirtIO_MMIO_Offset::DriverFeaturesSelect
                    // | VirtIO_MMIO_Offset::GuestPageSize => 0 as u32, // legacy
                    | VirtIO_MMIO_Offset::QueueSelect
                    | VirtIO_MMIO_Offset::QueueNum
                    // | VirtIO_MMIO_Offset::QueueAligne => 0 as u32, // legacy
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
    }

    pub fn write(&mut self, offset: u64, value: u32) {
        let vdev = unsafe { self.device.as_mut_unchecked() };

        if !check_align::<u32>(offset) {
            // will be checked in mmio.
            unreachable!()
        }

        let offset_type = VirtIO_MMIO_Offset::try_from(offset);
        match offset_type {
            Err(error) => {
                error!(
                    "VirtIO: write of unimplemented register: {:#x}, {}",
                    offset, error
                );
            }
            Ok(offset_type) => match offset_type {
                VirtIO_MMIO_Offset::DeviceFeaturesSelect => self.host_features_sel = value & 0x1,
                VirtIO_MMIO_Offset::DriverFeatures => {
                    let feature = (value as u64) << self.host_features_sel;
                    self.guest_features |= feature;
                }
                VirtIO_MMIO_Offset::DriverFeaturesSelect => {
                    self.guest_features_sel = value & 0x1;
                }
                // VirtIO_MMIO_Offset::GUEST_PAGE_SIZE => {}, // legacy
                VirtIO_MMIO_Offset::QueueSelect => {
                    vdev.set_queue_num(value);
                }
                VirtIO_MMIO_Offset::QueueNum => {
                    vdev.set_queue_num(value);
                }
                // VirtIO_MMIO_Offset::QueueAlign => {}, // legacy
                // VirtIO_MMIO_Offset::QueuePFN => {}, // legacy
                VirtIO_MMIO_Offset::QueueReady => {
                    let q = &mut self.queues[self.queue_select as usize];
                    vdev.set_desc(q.desc);
                    vdev.set_avail(q.avail);
                    vdev.set_used(q.used);
                    q.enable = true;
                }
                VirtIO_MMIO_Offset::QueueNotify => {
                    let queue_idx = value;
                    if queue_idx < vdev.get_num_of_queue() {
                        vdev.notify(queue_idx);
                    }
                }
                VirtIO_MMIO_Offset::InterruptAck => {
                    vdev.isr()
                        .fetch_and(value as u8, std::sync::atomic::Ordering::AcqRel);
                    vdev.update_irq();
                }
                VirtIO_MMIO_Offset::Status => {
                    if let Some(new_status) = VirtIODeviceStatus::from_bits(value as u8) {
                        // if (new_status & VirtIODeviceStatus::DRIVER_OK).is_empty() {
                        //     // virtio_mmio_stop_ioeventfd(proxy);
                        // }
                        if !(new_status & VirtIODeviceStatus::FEATURES_OK).is_empty() {
                            vdev.set_feature(self.guest_features)
                        }
                        *vdev.status() |= new_status.bits();
                    }
                }
                VirtIO_MMIO_Offset::QueueDescLow => {
                    let q = &mut self.queues[self.queue_select as usize];
                    q.desc |= value as u64;
                }
                VirtIO_MMIO_Offset::QueueDescHigh => {
                    let q = &mut self.queues[self.queue_select as usize];
                    q.desc |= (value as u64) << 32;
                }
                VirtIO_MMIO_Offset::QueueAvailLow => {
                    let q = &mut self.queues[self.queue_select as usize];
                    q.avail |= value as u64;
                }
                VirtIO_MMIO_Offset::QueueAvailHigh => {
                    let q = &mut self.queues[self.queue_select as usize];
                    q.avail |= (value as u64) << 32;
                }
                VirtIO_MMIO_Offset::QueueUsedLow => {
                    let q = &mut self.queues[self.queue_select as usize];
                    q.used |= value as u64;
                }
                VirtIO_MMIO_Offset::QueueUsedHigh => {
                    let q = &mut self.queues[self.queue_select as usize];
                    q.used |= (value as u64) << 32;
                }
                VirtIO_MMIO_Offset::MagicValue
                | VirtIO_MMIO_Offset::Version
                | VirtIO_MMIO_Offset::DeviceId
                | VirtIO_MMIO_Offset::VendorId
                | VirtIO_MMIO_Offset::DeviceFeatures
                | VirtIO_MMIO_Offset::QueueNumMax
                | VirtIO_MMIO_Offset::InterruptStatus
                | VirtIO_MMIO_Offset::ConfigGeneration => {
                    error!("VirtIO: write to read-only register: {:#x}", offset);
                }
                VirtIO_MMIO_Offset::SharedMemBaseHigh
                | VirtIO_MMIO_Offset::SharedMemBaseLow
                | VirtIO_MMIO_Offset::SharedMemLenHigh
                | VirtIO_MMIO_Offset::SharedMemLenLow
                | VirtIO_MMIO_Offset::SharedMemSelect => {
                    error!("VirtIO: DO NOT allow shared memory.");
                }
            },
        };
    }
}

#[cfg(test)]
mod test {
    use core::slice;
    use std::{
        fs::OpenOptions,
        io::{Read, Seek},
    };

    use super::*;
    use crate::{
        device::virtio::{
            virtio_blk::{
                VirtIOBlkDevice, VirtIOBlkReqStatus, VirtIOBlockFeature, VirtioBlkReq,
                VirtioBlkReqType, VirtioBlkStatus,
            },
            virtio_queue::{
                VirtQueueAvail, VirtQueueAvailFlag, VirtQueueDesc, VirtQueueDescFlag,
                VirtQueueUsed, VirtQueueUsedFlag,
            },
        },
        ram::Ram,
        ram_config,
    };

    const QUEUE_NUM: usize = 8;
    const DESC_NUM: usize = 16;

    #[test]
    fn test_mmio_blk_device() {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open("./tmp/test_mmio_blk_device.txt")
            .unwrap();

        let mut ram = Ram::new();
        let ram_base = &mut ram[0] as *mut u8;
        let mut virt_device = VirtIOBlkDevice::new(
            "VirtIO Block 0".to_string(),
            ram_base,
            0,
            file.try_clone().unwrap(),
        );

        virt_device = virt_device
            .add_host_feature(VirtIOBlockFeature::BlockSize)
            .add_host_feature(VirtIOBlockFeature::Flush);

        let mut virtio_mmio_device = VirtIOMMIO::new(Rc::new(UnsafeCell::new(virt_device)));
        // virt_device.set_queue_num(QUEUE_NUM as u32);
        virtio_mmio_device.write(
            VirtIO_MMIO_Offset::Status as u64,
            VirtIODeviceStatus::ACKNOWLEDGE.bits() as u32,
        );
        virtio_mmio_device.write(
            VirtIO_MMIO_Offset::Status as u64,
            VirtIODeviceStatus::DRIVER.bits() as u32,
        );
        virtio_mmio_device.write(VirtIO_MMIO_Offset::DriverFeaturesSelect as u64, 0);
        virtio_mmio_device.write(
            VirtIO_MMIO_Offset::DriverFeatures as u64,
            VirtIOBlockFeature::BlockSize as u32,
        );
        virtio_mmio_device.write(
            VirtIO_MMIO_Offset::DriverFeatures as u64,
            ((VirtIOBlockFeature::Flush as u64) >> 32) as u32,
        );
        virtio_mmio_device.write(
            VirtIO_MMIO_Offset::Status as u64,
            VirtIODeviceStatus::DRIVER_OK.bits() as u32,
        );
        virtio_mmio_device.write(
            VirtIO_MMIO_Offset::Status as u64,
            VirtIODeviceStatus::FEATURES_OK.bits() as u32,
        );
        let status = virtio_mmio_device.read(VirtIO_MMIO_Offset::Status as u64);
        assert!(status & VirtIODeviceStatus::DRIVER_OK.bits() as u32 != 0);

        virtio_mmio_device.write(VirtIO_MMIO_Offset::QueueSelect as u64, 0);
        virtio_mmio_device.write(VirtIO_MMIO_Offset::QueueNum as u64, QUEUE_NUM as u32);

        let virtq_desc_base = 0x8000_2000 as u64;
        let virtq_avail_base = 0x8000_2100 + ((QUEUE_NUM + 2) * size_of::<u16>()) as u64;
        let virtq_used_base = 0x8000_2200 + (QUEUE_NUM * size_of::<VirtQueueUsed>() + 4) as u64;
        // virt_device.set_avail(virtq_avail_base);
        // virt_device.set_desc(virtq_desc_base);
        // virt_device.set_used(virtq_used_base);
        virtio_mmio_device.write(
            VirtIO_MMIO_Offset::QueueAvailLow as u64,
            virtq_avail_base as u32,
        );
        virtio_mmio_device.write(
            VirtIO_MMIO_Offset::QueueAvailHigh as u64,
            (virtq_avail_base >> 32) as u32,
        );
        virtio_mmio_device.write(
            VirtIO_MMIO_Offset::QueueDescLow as u64,
            virtq_desc_base as u32,
        );
        virtio_mmio_device.write(
            VirtIO_MMIO_Offset::QueueDescHigh as u64,
            (virtq_desc_base >> 32) as u32,
        );
        virtio_mmio_device.write(
            VirtIO_MMIO_Offset::QueueUsedLow as u64,
            virtq_used_base as u32,
        );
        virtio_mmio_device.write(
            VirtIO_MMIO_Offset::QueueUsedHigh as u64,
            (virtq_used_base >> 32) as u32,
        );
        virtio_mmio_device.write(VirtIO_MMIO_Offset::QueueReady as u64, 0x01);

        // Description Table.
        let virt_queue_desc = unsafe {
            slice::from_raw_parts_mut(
                &mut ram[(virtq_desc_base - ram_config::BASE_ADDR) as usize] as *mut u8
                    as *mut VirtQueueDesc,
                DESC_NUM,
            )
        };

        // Available Ring.
        let virtq_avail = &mut ram[(virtq_avail_base - ram_config::BASE_ADDR) as usize] as *mut u8
            as *mut VirtQueueAvail;
        let virtq_avail = unsafe { virtq_avail.as_mut().unwrap() };
        virtq_avail.init(VirtQueueAvailFlag::Default);
        virtq_avail.idx_atomic_add(1);
        let avail_ring = VirtQueueAvail::mut_ring(virtq_avail as *mut _ as u64, QUEUE_NUM as u32);

        // Used Ring.
        let virtq_used = &mut ram[(virtq_used_base - ram_config::BASE_ADDR) as usize] as *mut u8
            as *mut VirtQueueUsed;
        let virtq_used = unsafe { virtq_used.as_mut().unwrap() };
        virtq_used.init(VirtQueueUsedFlag::Default);
        let _used_ring = virtq_used.ring(QUEUE_NUM as u32);

        // Write Available Ring.
        avail_ring[0] = 0;

        // header
        let desc0 = &mut virt_queue_desc[0];
        let desc0_buf_addr = 0x8000_2300;
        desc0.init(
            0x8000_2300,
            size_of::<VirtioBlkReq>() as u32,
            VirtQueueDescFlag::VIRTQ_DESC_F_NEXT,
            1,
        );
        let req = &mut ram[(desc0_buf_addr - ram_config::BASE_ADDR) as usize] as *mut u8
            as *mut VirtioBlkReq;
        let req = unsafe { req.as_mut().unwrap() };
        *req = VirtioBlkReq::new(VirtioBlkReqType::Out, 0);

        let desc1 = &mut virt_queue_desc[1];
        let desc1_buf_addr = 0x8000_2400;
        desc1.init(0x8000_2400, 0x200, VirtQueueDescFlag::VIRTQ_DESC_F_NEXT, 2);
        let desc_buf = unsafe {
            slice::from_raw_parts_mut(
                &mut ram[(desc1_buf_addr - ram_config::BASE_ADDR) as usize] as *mut u8,
                0x200,
            )
        };
        for i in 0..0x200 {
            desc_buf[i] = (i * i) as u8;
        }

        let desc2 = &mut virt_queue_desc[2];
        let desc2_buf_addr = 0x8000_2310;
        desc2.init(
            0x8000_2310,
            size_of::<VirtioBlkStatus>() as u32, // 1 byte
            VirtQueueDescFlag::empty(),
            0,
        );
        let desc_status = unsafe {
            (&mut ram[(desc2_buf_addr - ram_config::BASE_ADDR) as usize] as *mut u8
                as *mut VirtioBlkStatus)
                .as_mut()
                .unwrap()
        };

        // manage request.
        // let t = virt_device.manage_one_request();
        // assert_eq!(t, true);
        virtio_mmio_device.write(VirtIO_MMIO_Offset::QueueNotify as u64, 0x00);

        assert_eq!(desc_status.status, VirtIOBlkReqStatus::Ok as u8);
        assert_eq!(desc_buf[0], 0);

        // let used_ring = virt_device.queue().get_used_ring();
        // let used_index = used_ring.get_index();
        // assert_eq!(used_index, 1);
        // used_ring.index_add(1);

        // let used_elem = used_ring.ring(QUEUE_NUM as u32)[0];
        // assert_eq!(used_elem.get_len(), 0x200);
        // assert_eq!(used_elem.get_id(), 0);

        let mut buf: [u8; 512] = [0u8; 512];
        file.seek(std::io::SeekFrom::Start(0)).unwrap();
        file.read(&mut buf).unwrap();
        assert_eq!(buf[93], (93 * 93) as u8);
    }
}
