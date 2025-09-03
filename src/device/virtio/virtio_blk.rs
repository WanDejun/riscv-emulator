use std::sync::atomic::AtomicU8;

use crate::device::virtio::virtio_mmio::VirtIO_MMIO_Offset;

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[rustfmt::skip]
pub enum VirtIOBlockFeature {
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
pub struct VirtioBlkGeometry {
    pub cylinders: u16,
    pub heads: u8,
    pub sectors: u8,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct VirtioBlkTopology {
    pub physical_block_exp: u8,
    pub alignment_offset: u8,
    pub min_io_size: u16,
    pub opt_io_size: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
#[rustfmt::skip]
pub struct VirtioBlkConfig {
    pub capacity: u64,                      // 0x00: Size of the block device (in 512-byte sectors)
    pub size_max: u32,                      // 0x08: Maximum segment size (if VIRTIO_BLK_F_SIZE_MAX)
    pub seg_max: u32,                       // 0x0c: Maximum number of segments (if VIRTIO_BLK_F_SEG_MAX)
    pub geometry: VirtioBlkGeometry,        // 0x10: Disk geometry (if VIRTIO_BLK_F_GEOMETRY)
    pub blk_size: u32,                      // 0x14: Block size of device (if VIRTIO_BLK_F_BLK_SIZE)
    pub topology: VirtioBlkTopology,        // 0x18: Topology information (if VIRTIO_BLK_F_TOPOLOGY)
    pub writeback: u8,                      // 0x1c: Writeback mode (if VIRTIO_BLK_F_CONFIG_WCE)
    pub unused0: [u8; 3],                   // 0x1d: Padding
    pub num_queues: u16,                    // 0x20: Number of queues (if VIRTIO_BLK_F_MQ)
    pub unused1: [u8; 6],                   // 0x22: Padding
    pub max_discard_sectors: u32,           // 0x28: Max discard sectors (if VIRTIO_BLK_F_DISCARD)
    pub max_discard_seg: u32,               // 0x2c: Max discard segments (if VIRTIO_BLK_F_DISCARD)
    pub discard_sector_alignment: u32,      // 0x30: Discard sector alignment (if VIRTIO_BLK_F_DISCARD)
    pub max_write_zeroes_sectors: u32,      // 0x34: Max write zeroes sectors (if VIRTIO_BLK_F_WRITE_ZEROES)
    pub max_write_zeroes_seg: u32,          // 0x38: Max write zeroes segments (if VIRTIO_BLK_F_WRITE_ZEROES)
    pub write_zeroes_may_unmap: u8,         // 0x3c: Write zeroes may unmap (if VIRTIO_BLK_F_WRITE_ZEROES)
    pub unused2: [u8; 3],                   // 0x3d: Padding
    pub max_secure_erase_sectors: u32,      // 0x40: Max secure erase sectors (if VIRTIO_BLK_F_SECURE_ERASE)
    pub max_secure_erase_seg: u32,          // 0x44: Max secure erase segments (if VIRTIO_BLK_F_SECURE_ERASE)
    pub secure_erase_sector_alignment: u32, // 0x48: Secure erase sector alignment (if VIRTIO_BLK_F_SECURE_ERASE)
}

/// Virtio block request types
#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum VirtioBlkReqType {
    In = 0,
    Out = 1,
    Flush = 4,
    GetId = 8,
    Discard = 11,
    WriteZeroes = 13,
}

pub struct VirtIOBlkDevice {
    memory_mapped_register: [u32; 256],
    pub name: String,
    pub status: u8,
    pub isr: AtomicU8,

    pub device_id: u16,
    pub host_feature: u64,

    pub generation: u32,
}

impl VirtIOBlkDevice {
    pub fn new(name: String, device_id: u16) -> Self {
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
        }
    }
}
