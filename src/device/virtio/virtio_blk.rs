use core::slice;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    sync::atomic::AtomicU8,
};

use log::error;
use num_enum::TryFromPrimitive;

use crate::{
    device::virtio::{
        virtio_device::{DEVICE_ID_ALLOCTOR, VirtIODeviceTrait},
        virtio_mmio::VirtIODeviceStatus,
        virtio_queue::{VirtQueue, VirtQueueDesc},
    },
    emulator_panic,
};

pub(super) const SECTOR_SIZE: usize = 512;

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

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct VirtioBlkGeometry {
    pub(crate) cylinders: u16,
    pub(crate) heads: u8,
    pub(crate) sectors: u8,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct VirtioBlkTopology {
    pub(crate) physical_block_exp: u8,
    pub(crate) alignment_offset: u8,
    pub(crate) min_io_size: u16,
    pub(crate) opt_io_size: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
#[rustfmt::skip]
pub(crate) struct VirtioBlkConfig {
    pub(crate) capacity: u64,                      // 0x00: Size of the block device (in 512-byte sectors)
    pub(crate) size_max: u32,                      // 0x08: Maximum segment size        (if VIRTIO_BLK_F_SIZE_MAX)
    pub(crate) seg_max: u32,                       // 0x0c: Maximum number of segments  (if VIRTIO_BLK_F_SEG_MAX)
    pub(crate) geometry: VirtioBlkGeometry,        // 0x10: Disk geometry               (if VIRTIO_BLK_F_GEOMETRY)
    pub(crate) blk_size: u32,                      // 0x14: Block size of device        (if VIRTIO_BLK_F_BLK_SIZE)
    pub(crate) topology: VirtioBlkTopology,        // 0x18: Topology information        (if VIRTIO_BLK_F_TOPOLOGY)
    pub(crate) writeback: u8,                      // 0x1c: Writeback mode              (if VIRTIO_BLK_F_CONFIG_WCE)
    pub(crate) unused0: [u8; 3],                   // 0x1d: Padding
    pub(crate) num_queues: u16,                    // 0x20: Number of queues            (if VIRTIO_BLK_F_MQ)
    pub(crate) unused1: [u8; 6],                   // 0x22: Padding
    pub(crate) max_discard_sectors: u32,           // 0x28: Max discard sectors         (if VIRTIO_BLK_F_DISCARD)
    pub(crate) max_discard_seg: u32,               // 0x2c: Max discard segments        (if VIRTIO_BLK_F_DISCARD)
    pub(crate) discard_sector_alignment: u32,      // 0x30: Discard sector alignment    (if VIRTIO_BLK_F_DISCARD)
    pub(crate) max_write_zeroes_sectors: u32,      // 0x34: Max write zeroes sectors    (if VIRTIO_BLK_F_WRITE_ZEROES)
    pub(crate) max_write_zeroes_seg: u32,          // 0x38: Max write zeroes segments   (if VIRTIO_BLK_F_WRITE_ZEROES)
    pub(crate) write_zeroes_may_unmap: u8,         // 0x3c: Write zeroes may unmap      (if VIRTIO_BLK_F_WRITE_ZEROES)
    pub(crate) unused2: [u8; 3],                   // 0x3d: Padding
    pub(crate) max_secure_erase_sectors: u32,      // 0x40: Max secure erase sectors        (if VIRTIO_BLK_F_SECURE_ERASE)
    pub(crate) max_secure_erase_seg: u32,          // 0x44: Max secure erase segments       (if VIRTIO_BLK_F_SECURE_ERASE)
    pub(crate) secure_erase_sector_alignment: u32, // 0x48: Secure erase sector alignment   (if VIRTIO_BLK_F_SECURE_ERASE)
}

impl VirtioBlkConfig {
    pub(crate) fn new(capacity: u64) -> Self {
        let mut config = Self::default();
        config.capacity = capacity;
        config.blk_size = SECTOR_SIZE as u32;
        config
    }

    pub(crate) fn into_slice(&self) -> &[u32] {
        unsafe {
            slice::from_raw_parts(
                self as *const Self as *const u32,
                size_of::<VirtioBlkConfig>() / 4,
            )
        }
    }

    pub(crate) fn into_slice_mut(&mut self) -> &mut [u32] {
        unsafe {
            slice::from_raw_parts_mut(
                self as *mut Self as *mut u32,
                size_of::<VirtioBlkConfig>() / 4,
            )
        }
    }
}

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
    pub(crate) name: &'static str,
    pub(crate) status: u8,
    pub(crate) isr: AtomicU8,
    pub(crate) device_id: u16,

    host_feature: u64,
    guest_feature: u64,

    pub(crate) generation: u32,
    ram_base_raw: usize,

    file: File, // the file that is bound to this device

    queue: VirtQueue,
    pub(super) config_region: VirtioBlkConfig,
}

impl VirtIOBlkDevice {
    pub(crate) fn new(
        name: &'static str,
        ram_base_raw: *mut u8,
        device_id: u16,
        file_path: String,
    ) -> Self {
        let mut file;
        if let Ok(file_result) = OpenOptions::new()
            .read(true)
            .write(true)
            .append(false)
            .create(false)
            .open(file_path.as_str())
        {
            file = file_result;
        } else {
            emulator_panic!("Can not find file: {}.", file_path);
        }
        let size = file.seek(SeekFrom::End(0)).unwrap();

        Self {
            name,
            status: 0,
            device_id,

            isr: AtomicU8::new(0),

            host_feature: 0,
            guest_feature: 0,

            generation: 0,
            ram_base_raw: ram_base_raw as usize,

            file,

            queue: VirtQueue::new(ram_base_raw, 0), // will be set later
            config_region: VirtioBlkConfig::new(size.div_ceil(SECTOR_SIZE as u64)),
        }
    }

    pub(crate) fn bound_file(&mut self, file: File) {
        self.file = file;
    }
    pub fn add_host_feature(mut self, new_feature: VirtIOBlockFeature) -> Self {
        self.host_feature |= new_feature as u64;
        self
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
    fn set_feature(&mut self, feature: u64) {
        if self.host_feature & feature != feature {
            self.status &= !(VirtIODeviceStatus::DRIVER_OK.bits())
        } else {
            self.guest_feature = feature;
        }
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
                            Self::read_blk(&mut self.file, buf, sector * SECTOR_SIZE as u64)
                        }
                        VirtioBlkReqType::Out => {
                            Self::write_blk(&mut self.file, buf, sector * SECTOR_SIZE as u64)
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

    fn read_config(&mut self, idx: u64) -> u32 {
        self.config_region.into_slice()[idx as usize]
    }

    fn write_config(&mut self, idx: u64, data: u32) {
        self.config_region.into_slice_mut()[idx as usize] = data
    }

    fn get_poll_enent(&mut self) -> Option<crate::async_poller::PollingEvent> {
        None
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

pub struct VirtIOBlkDeviceBuilder {
    device: VirtIOBlkDevice,
}

impl VirtIOBlkDeviceBuilder {
    pub fn new(ram_base_raw: *mut u8, file: String) -> Self {
        let device_id = DEVICE_ID_ALLOCTOR.lock().unwrap().alloc();
        Self {
            device: VirtIOBlkDevice::new(
                "Unnamed VirtIO Block Device",
                ram_base_raw,
                device_id,
                file,
            ),
        }
    }

    pub fn name(mut self, name: &'static str) -> Self {
        self.device.name = name;
        self
    }

    pub fn host_feature(mut self, feature: VirtIOBlockFeature) -> Self {
        self.device.host_feature |= feature as u64;
        self
    }

    pub fn generation(mut self, generation: u32) -> Self {
        self.device.generation = generation;
        self
    }

    pub fn get(self) -> VirtIOBlkDevice {
        self.device
    }
}

#[cfg(test)]
pub fn init_block_file<'a, F>(path: &str, blk_num: u64, mut f: F) -> File
where
    F: FnMut(usize) -> &'a [u8],
{
    use std::{fs::create_dir_all, path::Path};
    let parent_dir = Path::new(path).parent().unwrap();
    create_dir_all(parent_dir).unwrap();

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .unwrap();
    // let write_buf: [u8; SECTOR_SIZE] = [0; SECTOR_SIZE];
    for i in 0..blk_num {
        let write_buf = f(i as usize);
        assert_eq!(write_buf.len(), SECTOR_SIZE);
        file.write_all(write_buf).unwrap();
    }

    file
}

#[cfg(test)]
mod test {
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
        // let mut file = OpenOptions::new()
        //     .read(true)
        //     .write(true)
        //     .create(true)
        //     .truncate(true)
        //     .open("./tmp/test_file_read_write.txt")
        //     .unwrap();
        let write_buf: [u8; SECTOR_SIZE] = [0xAB; SECTOR_SIZE];
        let offset = 0;
        let mut file = init_block_file("./tmp/test_file_read_write.txt", 1, |_| &write_buf);

        // 测试写入
        let write_len = VirtIOBlkDevice::write_blk(&mut file, &write_buf, offset);
        assert_eq!(write_len, SECTOR_SIZE as u32);

        let mut file_copy = file.try_clone().unwrap();
        // 测试读取
        let mut read_buf: [u8; SECTOR_SIZE] = [0u8; SECTOR_SIZE];
        let read_len = VirtIOBlkDevice::read_blk(&mut file_copy, &mut read_buf, offset);
        assert_eq!(read_len, SECTOR_SIZE as u32);
        assert_eq!(read_buf, write_buf);
    }

    #[test]
    fn test_blk_read() {
        let mut buf: [u8; SECTOR_SIZE] = [0u8; SECTOR_SIZE];
        buf[0xff] = 0x55;
        let file_name = String::from("./tmp/test_blk_read.txt");
        let _ = init_block_file(&file_name, 1, |_| &buf);

        let mut ram = Ram::new();
        let ram_base = &mut ram[0] as *mut u8;
        let mut virt_device = VirtIOBlkDevice::new("VirtIO Block 0", ram_base, 0, file_name);
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
        // init file.
        let mut buf: [u8; SECTOR_SIZE] = [0u8; SECTOR_SIZE];
        buf[0xff] = 0x55;
        let file_name = String::from("./tmp/test_blk_write.txt");
        let mut file = init_block_file(file_name.as_str(), 1, |_| &buf);

        let mut ram = Ram::new();
        let ram_base = &mut ram[0] as *mut u8;
        let mut virt_device = VirtIOBlkDevice::new("VirtIO Block 0", ram_base, 0, file_name);
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

        let mut buf: [u8; SECTOR_SIZE] = [0u8; SECTOR_SIZE];
        file.seek(std::io::SeekFrom::Start(0)).unwrap();
        file.read(&mut buf).unwrap();
        assert_eq!(buf[93], (93 * 93) as u8);
    }
}
