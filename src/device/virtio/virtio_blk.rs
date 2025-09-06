use core::slice;
use std::{
    fs::File,
    io::{Read, Seek, Write},
    sync::atomic::AtomicU8,
};

use log::error;
use num_enum::TryFromPrimitive;

use crate::device::virtio::{
    virtio_device::VirtIODeviceTrait,
    virtio_mmio::VirtIO_MMIO_Offset,
    virtio_queue::{VirtQueue, VirtQueueDesc},
};

const SECTOR_SIZE: u64 = 512;

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[rustfmt::skip]
pub(crate) enum VirtIOBlockFeature {
    SizeMax      = 1 << 1,   // Maximum segment size supported
    SegMax       = 1 << 2,   // Maximum number of segments supported
    Geometry     = 1 << 4,   // Disk geometry available
    Ro           = 1 << 5,   // Device is read-only
    BlockSize    = 1 << 6,   // Block size available
    Flush        = 1 << 9,   // Cache flush command supported
    Topology     = 1 << 10,  // Device exports topology information
    ConfigWce    = 1 << 11,  // Writeback mode available in config
    Multiqueue   = 1 << 12,  // Device supports multiqueue.
    Discard      = 1 << 13,  // Discard command supported
    WriteZeroes  = 1 << 14,  // Write zeroes command supported
    Lifetime     = 1 << 15,  // Device supports providing storage lifetime information.
    SecureErase  = 1 << 16,  // Secure erase supported
}

// #[repr(C, packed)]
// #[derive(Debug, Clone, Copy, Default)]
// pub(crate) struct VirtioBlkGeometry {
//     pub(crate) cylinders: u16,
//     pub(crate) heads: u8,
//     pub(crate) sectors: u8,
// }

// #[repr(C, packed)]
// #[derive(Debug, Clone, Copy, Default)]
// pub struct VirtioBlkTopology {
//     pub physical_block_exp: u8,
//     pub alignment_offset: u8,
//     pub min_io_size: u16,
//     pub opt_io_size: u32,
// }

// #[repr(C, packed)]
// #[derive(Debug, Clone, Copy, Default)]
// #[rustfmt::skip]
// pub struct VirtioBlkConfig {
//     pub capacity: u64,                      // 0x00: Size of the block device (in 512-byte sectors)
//     pub size_max: u32,                      // 0x08: Maximum segment size (if VIRTIO_BLK_F_SIZE_MAX)
//     pub seg_max: u32,                       // 0x0c: Maximum number of segments (if VIRTIO_BLK_F_SEG_MAX)
//     pub geometry: VirtioBlkGeometry,        // 0x10: Disk geometry (if VIRTIO_BLK_F_GEOMETRY)
//     pub blk_size: u32,                      // 0x14: Block size of device (if VIRTIO_BLK_F_BLK_SIZE)
//     pub topology: VirtioBlkTopology,        // 0x18: Topology information (if VIRTIO_BLK_F_TOPOLOGY)
//     pub writeback: u8,                      // 0x1c: Writeback mode (if VIRTIO_BLK_F_CONFIG_WCE)
//     pub unused0: [u8; 3],                   // 0x1d: Padding
//     pub num_queues: u16,                    // 0x20: Number of queues (if VIRTIO_BLK_F_MQ)
//     pub unused1: [u8; 6],                   // 0x22: Padding
//     pub max_discard_sectors: u32,           // 0x28: Max discard sectors (if VIRTIO_BLK_F_DISCARD)
//     pub max_discard_seg: u32,               // 0x2c: Max discard segments (if VIRTIO_BLK_F_DISCARD)
//     pub discard_sector_alignment: u32,      // 0x30: Discard sector alignment (if VIRTIO_BLK_F_DISCARD)
//     pub max_write_zeroes_sectors: u32,      // 0x34: Max write zeroes sectors (if VIRTIO_BLK_F_WRITE_ZEROES)
//     pub max_write_zeroes_seg: u32,          // 0x38: Max write zeroes segments (if VIRTIO_BLK_F_WRITE_ZEROES)
//     pub write_zeroes_may_unmap: u8,         // 0x3c: Write zeroes may unmap (if VIRTIO_BLK_F_WRITE_ZEROES)
//     pub unused2: [u8; 3],                   // 0x3d: Padding
//     pub max_secure_erase_sectors: u32,      // 0x40: Max secure erase sectors (if VIRTIO_BLK_F_SECURE_ERASE)
//     pub max_secure_erase_seg: u32,          // 0x44: Max secure erase segments (if VIRTIO_BLK_F_SECURE_ERASE)
//     pub secure_erase_sector_alignment: u32, // 0x48: Secure erase sector alignment (if VIRTIO_BLK_F_SECURE_ERASE)
// }

// ======================================
//      Virtio block request types
// ======================================
#[repr(u32)]
#[derive(Debug, Clone, Copy, TryFromPrimitive)]
pub(crate) enum VirtioBlkReqType {
    In = 0,
    Out = 1,
    Flush = 4,
    GetId = 8,
    GetLifetime = 10,
    Discard = 11,
    WriteZeroes = 13,
    SecureErase = 14,
    Unsupported = 0xFFFFFFFF,
}

// Virtio block request header (0x10 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub(super) struct VirtioBlkReq {
    request_type: u32, // (VirtioBlkReqStatus)
    reserved: u32,
    sector: u64,
}

#[cfg(test)]
impl VirtioBlkReq {
    pub(super) fn new(request_type: VirtioBlkReqType, sector: u64) -> Self {
        Self {
            request_type: request_type as u32,
            reserved: 0,
            sector,
        }
    }
}

struct VirtIOBlkData {
    data0: u8,
    // data1, data2, ..., dataN
}
impl VirtIOBlkData {
    fn as_mut_slice(&mut self, len: usize) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(&mut self.data0 as *mut u8, len) }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VirtIOBlkReqStatus {
    Ok = 0,
    IoErr = 1,
    Unsupported = 2,
    NotReady = 3,
}

pub(super) struct VirtioBlkStatus {
    pub(super) status: u8,
}
impl VirtioBlkStatus {
    fn write_status(&mut self, status: VirtIOBlkReqStatus) {
        self.status = status as u8;
    }
}

// ======================================
//          Virtio Block Device
// ======================================
pub(crate) struct VirtIOBlkDevice {
    memory_mapped_register: [u32; 256],
    pub(crate) name: String,
    pub(crate) status: u8,
    pub(crate) isr: AtomicU8,
    pub(crate) device_id: u16,

    pub(crate) host_feature: u64,

    pub(crate) generation: u32,
    ram_base_raw: usize,

    file: File, // the file that is bound to this device

    queue: VirtQueue,
}

impl VirtIOBlkDevice {
    pub(crate) fn new(name: String, ram_base_raw: *mut u8, device_id: u16, file: File) -> Self {
        let mut register = [0; 256];
        register[VirtIO_MMIO_Offset::MagicValue as usize] = 0x74726976; // Magic Value
        register[VirtIO_MMIO_Offset::Version as usize] = 0x2; // VirtIO Version
        register[VirtIO_MMIO_Offset::DeviceId as usize] = 0x2; // Block device
        register[VirtIO_MMIO_Offset::VendorId as usize] = 0x1af4; // Vendor 

        register[VirtIO_MMIO_Offset::Status as usize] = 0x00; // Status

        Self {
            memory_mapped_register: register,
            name,
            status: 0,
            isr: AtomicU8::new(0),

            device_id,
            host_feature: 0,

            generation: 0,
            ram_base_raw: ram_base_raw as usize,

            file,

            queue: VirtQueue::new(ram_base_raw, 0), // will be set later
        }
    }

    pub(crate) fn bound_file(&mut self, file: File) {
        self.file = file;
    }

    fn write_blk(file: &mut File, buf: &[u8], offset: u64) -> u32 {
        file.seek(std::io::SeekFrom::Start(offset)).unwrap();
        match file.write_all(buf) {
            Ok(_) => buf.len() as u32,
            Err(_) => 0,
        }
    }

    fn read_blk(file: &mut File, buf: &mut [u8], offset: u64) -> u32 {
        file.seek(std::io::SeekFrom::Start(offset)).unwrap();
        match file.read(buf) {
            Ok(len) => len as u32,
            #[cfg(not(test))]
            Err(_) => 0,
            #[cfg(test)]
            Err(mes) => panic!("{}", mes),
        }
    }

    fn manage_request_header(ram_base_raw: usize, desc: &VirtQueueDesc) -> (VirtioBlkReqType, u64) {
        let req = unsafe {
            desc.get_request_package::<VirtioBlkReq>(ram_base_raw)
                .as_mut()
                .unwrap()
        };

        VirtioBlkReqType::try_from(req.request_type)
            .map_or((VirtioBlkReqType::Unsupported, 0u64), |req_type| {
                (req_type, req.sector)
            })
    }
}

impl VirtIODeviceTrait for VirtIOBlkDevice {
    fn get_device_id(&self) -> u16 {
        self.device_id
    }
    fn status(&mut self) -> &mut u8 {
        &mut self.status
    }
    fn get_generation(&self) -> u32 {
        self.generation
    }

    fn isr(&mut self) -> &mut AtomicU8 {
        &mut self.isr
    }
    fn update_irq(&mut self) {
        // TODO!
        todo!()
    }

    fn get_host_feature(&self) -> u64 {
        self.host_feature
    }
    fn set_queue_num(&mut self, num: u32) {
        self.queue.set_queue_num(num);
    }
    fn queue_select(&self, _idx: u32) {
        // ONLY ONE QUEUE.
    }

    fn set_desc(&mut self, addr: u64) {
        self.queue.set_desc(addr);
    }
    fn set_avail(&mut self, addr: u64) {
        self.queue.set_avail(addr);
    }
    fn set_used(&mut self, addr: u64) {
        self.queue.set_used(addr);
    }

    fn manage_one_request(&mut self) -> bool {
        let mut req_type = VirtioBlkReqType::Unsupported;
        let mut sector: u64 = 0;
        let res = self
            .queue
            .manage_one_request(|desc: &VirtQueueDesc, idx: usize| match idx {
                0 => {
                    let t = Self::manage_request_header(self.ram_base_raw, desc);
                    req_type = t.0;
                    sector = t.1;
                    0
                }

                1 => {
                    let buf = unsafe {
                        slice::from_raw_parts_mut(
                            desc.get_request_package::<u8>(self.ram_base_raw),
                            desc.len as usize,
                        )
                    };

                    match req_type {
                        VirtioBlkReqType::In => {
                            Self::read_blk(&mut self.file, buf, sector * SECTOR_SIZE)
                        }
                        VirtioBlkReqType::Out => {
                            Self::write_blk(&mut self.file, buf, sector * SECTOR_SIZE)
                        }
                        VirtioBlkReqType::Flush => {
                            self.file.flush().unwrap();
                            0
                        }
                        _ => {
                            error!("virtio unsupport request: {:#?}", req_type);
                            let status_bit = unsafe {
                                desc.get_request_package::<VirtioBlkStatus>(self.ram_base_raw)
                                    .as_mut()
                                    .unwrap()
                            };
                            status_bit.write_status(VirtIOBlkReqStatus::Ok);
                            0
                        }
                    }
                }

                2 => {
                    let status_bit = unsafe {
                        desc.get_request_package::<VirtioBlkStatus>(self.ram_base_raw)
                            .as_mut()
                            .unwrap()
                    };
                    status_bit.write_status(VirtIOBlkReqStatus::Ok);
                    0
                }

                _ => {
                    error!(
                        "illigal virtio request: {:#?}. More than 3 description table",
                        req_type
                    );
                    0
                }
            });
        res
    }

    fn notify(&mut self, _idx: u32) {
        loop {
            if !self.manage_one_request() {
                break;
            }
        }
    }

    fn queue_ready(&self) -> bool {
        self.queue.ready()
    }

    fn get_num_of_queue(&self) -> u32 {
        1
    }
}

#[cfg(test)]
impl VirtIOBlkDevice {
    pub(crate) fn flush(&mut self) {
        self.file.flush().unwrap();
    }

    pub(crate) fn queue(&mut self) -> &mut VirtQueue {
        &mut self.queue
    }
}

#[cfg(test)]
mod test {
    use std::fs::OpenOptions;

    use crate::{
        device::virtio::virtio_queue::{
            VirtQueueAvail, VirtQueueAvailFlag, VirtQueueDescFlag, VirtQueueUsed, VirtQueueUsedFlag,
        },
        ram::Ram,
        ram_config,
    };

    use super::*;
    const QUEUE_NUM: usize = 8;
    const DESC_NUM: usize = QUEUE_NUM * 3; // each request need

    #[test]
    fn test_file_read_write() {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open("./tmp/test_file_read_write.txt")
            .unwrap();
        let write_buf: [u8; 512] = [0xAB; 512];
        let offset = 0;

        // 测试写入
        let write_len = VirtIOBlkDevice::write_blk(&mut file, &write_buf, offset);
        assert_eq!(write_len, 512);

        let mut file_copy = file.try_clone().unwrap();
        // 测试读取
        let mut read_buf: [u8; 512] = [0u8; 512];
        let read_len = VirtIOBlkDevice::read_blk(&mut file_copy, &mut read_buf, offset);
        assert_eq!(read_len, 512);
        assert_eq!(read_buf, write_buf);
    }

    #[test]
    fn test_blk_read() {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open("./tmp/test_blk_read.txt")
            .unwrap();
        let mut buf: [u8; 512] = [0u8; 512];
        buf[0xff] = 0x55;
        file.write_all(&buf).unwrap();

        let mut ram = Ram::new();
        let ram_base = &mut ram[0] as *mut u8;
        let mut virt_device = VirtIOBlkDevice::new("VirtIO Block 0".to_string(), ram_base, 0, file);
        virt_device.set_queue_num(QUEUE_NUM as u32);

        let virtq_desc_base = 0x8000_2000 as u64;
        let virtq_avail_base = 0x8000_2100 + ((QUEUE_NUM + 2) * size_of::<u16>()) as u64;
        let virtq_used_base = 0x8000_2200 + (QUEUE_NUM * size_of::<VirtQueueUsed>() + 4) as u64;
        virt_device.set_avail(virtq_avail_base);
        virt_device.set_desc(virtq_desc_base);
        virt_device.set_used(virtq_used_base);

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
        let avail_ring = VirtQueueAvail::mut_ring(virtq_avail as *mut _ as u64, QUEUE_NUM as u32);

        // Used Ring.
        let virtq_used = &mut ram[(virtq_used_base - ram_config::BASE_ADDR) as usize] as *mut u8
            as *mut VirtQueueUsed;
        let virtq_used = unsafe { virtq_used.as_mut().unwrap() };
        virtq_used.init(VirtQueueUsedFlag::Default);
        let _used_ring = virtq_used.ring(QUEUE_NUM as u32);

        // Write Available Ring.
        avail_ring[0] = 0;
        virtq_avail.idx_atomic_add(1);

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
        req.request_type = VirtioBlkReqType::In as u32;
        req.reserved = 0;
        req.sector = 0;

        let desc1 = &mut virt_queue_desc[1];
        let desc1_buf_addr = 0x8000_2400;
        desc1.init(0x8000_2400, 0x200, VirtQueueDescFlag::VIRTQ_DESC_F_NEXT, 2);
        let desc_buf = unsafe {
            slice::from_raw_parts_mut(
                &mut ram[(desc1_buf_addr - ram_config::BASE_ADDR) as usize] as *mut u8,
                0x200,
            )
        };

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
        let t = virt_device.manage_one_request();
        assert_eq!(t, true);

        assert_eq!(desc_status.status, VirtIOBlkReqStatus::Ok as u8);
        assert_eq!(desc_buf[0], 0);

        let used_ring = virt_device.queue.get_used_ring();
        let used_index = used_ring.get_index();
        assert_eq!(used_index, 1);
        // used_ring.index_add(1);

        let used_elem = used_ring.ring(QUEUE_NUM as u32)[0];
        assert_eq!(used_elem.get_len(), 0x200);
        assert_eq!(used_elem.get_id(), 0);
    }

    #[test]
    fn test_blk_write() {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open("./tmp/test_blk_write.txt")
            .unwrap();

        let mut ram = Ram::new();
        let ram_base = &mut ram[0] as *mut u8;
        let mut virt_device = VirtIOBlkDevice::new(
            "VirtIO Block 0".to_string(),
            ram_base,
            0,
            file.try_clone().unwrap(),
        );
        virt_device.set_queue_num(QUEUE_NUM as u32);

        let virtq_desc_base = 0x8000_2000 as u64;
        let virtq_avail_base = 0x8000_2100 + ((QUEUE_NUM + 2) * size_of::<u16>()) as u64;
        let virtq_used_base = 0x8000_2200 + (QUEUE_NUM * size_of::<VirtQueueUsed>() + 4) as u64;
        virt_device.set_avail(virtq_avail_base);
        virt_device.set_desc(virtq_desc_base);
        virt_device.set_used(virtq_used_base);

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
        let avail_ring = VirtQueueAvail::mut_ring(virtq_avail as *mut _ as u64, QUEUE_NUM as u32);

        // Used Ring.
        let virtq_used = &mut ram[(virtq_used_base - ram_config::BASE_ADDR) as usize] as *mut u8
            as *mut VirtQueueUsed;
        let virtq_used = unsafe { virtq_used.as_mut().unwrap() };
        virtq_used.init(VirtQueueUsedFlag::Default);
        let _used_ring = virtq_used.ring(QUEUE_NUM as u32);

        // Write Available Ring.
        avail_ring[0] = 0;
        virtq_avail.idx_atomic_add(1);

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
        req.request_type = VirtioBlkReqType::Out as u32;
        req.reserved = 0;
        req.sector = 0;

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
        let t = virt_device.manage_one_request();
        assert_eq!(t, true);

        assert_eq!(desc_status.status, VirtIOBlkReqStatus::Ok as u8);
        assert_eq!(desc_buf[0], 0);

        let used_ring = virt_device.queue.get_used_ring();
        let used_index = used_ring.get_index();
        assert_eq!(used_index, 1);
        // used_ring.index_add(1);

        let used_elem = used_ring.ring(QUEUE_NUM as u32)[0];
        assert_eq!(used_elem.get_len(), 0x200);
        assert_eq!(used_elem.get_id(), 0);

        let mut buf: [u8; 512] = [0u8; 512];
        file.seek(std::io::SeekFrom::Start(0)).unwrap();
        file.read(&mut buf).unwrap();
        assert_eq!(buf[93], (93 * 93) as u8);
    }
}
