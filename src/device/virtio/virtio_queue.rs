use core::slice;
use std::{ptr::null_mut, sync::atomic::AtomicU16};

use bitflags::bitflags;
use log::error;

use crate::ram_config;

// =====================================
//           VirtQueueDesc
// =====================================
bitflags! {
    pub(crate) struct VirtQueueDescFlag: u16 {
    /* This marks a buffer as continuing via the next field. */
    const VIRTQ_DESC_F_NEXT     = 1 << 0;

    /* This marks a buffer as device write-only (otherwise device read-only). */
    const VIRTQ_DESC_F_WRITE    = 1 << 1;

    /* This means the buffer contains a list of buffer descriptors. */
    const VIRTQ_DESC_F_INDIRECT = 1 << 2;
    }
}

#[repr(C)]
// 128 bits (0x10 bytes)
pub(crate) struct VirtQueueDesc {
    /* Address (guest-physical). */
    paddr: u64,
    /* Length. */
    pub(super) len: u32,
    /* The flags as indicated above. */
    flags: VirtQueueDescFlag,
    /* Next field if flags & NEXT */
    next: u16,
}

impl VirtQueueDesc {
    pub(crate) fn get_request_package<T>(&self, ram_base_raw: usize) -> *mut T {
        (self.paddr - ram_config::BASE_ADDR + ram_base_raw as u64) as *mut T
    }
}

#[cfg(test)]
impl VirtQueueDesc {
    pub(crate) fn init(&mut self, paddr: u64, len: u32, flags: VirtQueueDescFlag, next: u16) {
        self.paddr = paddr;
        self.len = len;
        self.flags = flags;
        self.next = next;
    }
}

pub(crate) struct VirtQueueDescHandle<'a> {
    table: &'a [VirtQueueDesc],
    ram_base: usize,
    idx: usize,
}

impl VirtQueueDescHandle<'_> {
    pub(crate) fn new(
        table: *const VirtQueueDesc,
        ram_base: usize,
        queue_num: u32,
        idx: usize,
    ) -> Self {
        Self {
            table: unsafe { slice::from_raw_parts(table, queue_num as usize) },
            ram_base,
            idx,
        }
    }

    fn get_indirect(&mut self) {
        let mut idx = self.idx;
        while self.table[idx]
            .flags
            .contains(VirtQueueDescFlag::VIRTQ_DESC_F_INDIRECT)
        {
            // Handle indirect descriptor case
            debug_assert_eq!(
                self.table[idx].len as usize % std::mem::size_of::<VirtQueueDesc>(),
                0
            );
            self.table = unsafe {
                slice::from_raw_parts(
                    (self.table[idx].paddr - self.ram_base as u64) as *const VirtQueueDesc,
                    self.table[idx].len as usize / std::mem::size_of::<VirtQueueDesc>(),
                )
            };
            idx = 0;
        }
    }

    pub(crate) fn get_entry_idx(&self) -> u32 {
        self.idx as u32
    }

    pub(crate) fn try_get(&mut self) -> Option<&VirtQueueDesc> {
        if self.idx < self.table.len() {
            // indirect.
            if self.table[self.idx]
                .flags
                .contains(VirtQueueDescFlag::VIRTQ_DESC_F_INDIRECT)
            {
                self.get_indirect();
            }

            // get current and update to next.
            let cur = self.idx;
            if self.table[cur]
                .flags
                .contains(VirtQueueDescFlag::VIRTQ_DESC_F_NEXT)
            {
                self.idx = self.table[cur].next as usize;
            } else {
                self.idx = self.table.len(); // mark as end.
            }
            Some(&self.table[cur])
        } else {
            None
        }
    }
}

// =====================================
//           VirtQueueAvail
// =====================================
#[repr(u16)]
#[derive(Debug, Clone, Copy)]
pub(crate) enum VirtQueueAvailFlag {
    Default = 0,     // Interrupt the device when a `UsedRing` is consumed.
    NoInterrupt = 1, // Driver will polling the `UsedRing`. Do not need to interrupt the device.
}

// (6 + QueueNum * 2) bytes
#[repr(C)]
pub(crate) struct VirtQueueAvail {
    flags: VirtQueueAvailFlag, // Written by Driver. (u16)
    idx: AtomicU16,            // Written by Driver.
    ring0: u16,
    /* ring1 ... */
    /* Only if VIRTIO_F_EVENT_IDX: used_event: u16; */
}

impl VirtQueueAvail {
    fn ring(&self, queue_num: u32) -> &[u16] {
        unsafe { std::slice::from_raw_parts(&self.ring0 as *const u16, queue_num as usize) }
    }

    fn try_get_desc_idx(&self, queue_num: u32, last_avail_idx: &mut u16) -> Option<u16> {
        let old_idx = *last_avail_idx;
        *last_avail_idx = (old_idx + 1) % queue_num as u16;

        if (old_idx as u32) < queue_num {
            Some(self.ring(queue_num)[old_idx as usize])
        } else {
            None
        }
    }
}

#[cfg(test)]
impl VirtQueueAvail {
    pub(crate) fn mut_ring(base: u64, queue_num: u32) -> &'static mut [u16] {
        unsafe { std::slice::from_raw_parts_mut((base + 4) as *mut u16, queue_num as usize) }
    }
    pub(crate) fn idx_add(&mut self, val: u16) {
        self.idx.fetch_add(val, std::sync::atomic::Ordering::AcqRel);
    }
    pub(crate) fn idx_store(&mut self, val: u16) {
        self.idx.store(val, std::sync::atomic::Ordering::Release);
    }
    pub(crate) fn init(&mut self, flag: VirtQueueAvailFlag) {
        self.flags = flag;
        self.idx.store(0, std::sync::atomic::Ordering::Relaxed);
    }
}

// =====================================
//           VirtQueueUsed
// =====================================
#[repr(u16)]
#[derive(Debug, Clone, Copy)]
pub(crate) enum VirtQueueUsedFlag {
    Default = 0,  // Notify the device when a `AvailRing` is used.
    NoNotify = 1, // Device will polling the `AvailRing`. Do not notify the device.
}

/* le32 is used here for ids for padding reasons. */
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct VirtQueueUsedElem {
    /* Index of start of used descriptor chain. */
    id: u32,
    /* Total length of the descriptor chain which was written to. */
    len: u32,
}
#[cfg(test)]
impl VirtQueueUsedElem {
    pub(crate) fn get_id(&self) -> u32 {
        self.id
    }
    pub(crate) fn get_len(&self) -> u32 {
        self.len
    }
}

#[repr(C)]
// (6 + QueueNum * 8) bytes
pub(crate) struct VirtQueueUsed {
    flags: VirtQueueUsedFlag, // defined by device.
    idx: AtomicU16,           // Written by Device. (Locked in VirtQueue).
    ring0: VirtQueueUsedElem,
    /* ring1 ... */
    /* Only if VIRTIO_F_EVENT_IDX: used_event: u16; */
}

impl VirtQueueUsed {
    pub(super) fn ring(&mut self, queue_num: u32) -> &mut [VirtQueueUsedElem] {
        unsafe {
            std::slice::from_raw_parts_mut(
                &mut self.ring0 as *mut VirtQueueUsedElem,
                queue_num as usize,
            )
        }
    }

    fn insert_used(&mut self, queue_num: u32, elem: VirtQueueUsedElem) {
        let mut idx = self.idx.load(std::sync::atomic::Ordering::Relaxed);
        let old_idx = idx;
        idx += 1;
        idx %= queue_num as u16;
        self.idx.store(idx, std::sync::atomic::Ordering::Relaxed);

        self.ring(queue_num)[old_idx as usize] = elem;
    }
}

#[cfg(test)]
impl VirtQueueUsed {
    pub(crate) fn init(&mut self, flag: VirtQueueUsedFlag) {
        self.flags = flag;
        self.idx.store(0, std::sync::atomic::Ordering::Relaxed);
    }
    pub(crate) fn get_index(&self) -> u16 {
        self.idx.load(std::sync::atomic::Ordering::Relaxed)
    }
    pub(crate) fn index_add(&self, val: u16) {
        self.idx
            .fetch_add(val, std::sync::atomic::Ordering::Release);
    }
}

/// Needs to be wrapped in a Mutex.
pub(crate) struct VirtQueue {
    queue_num: u32,
    ram_base_raw: *mut u8,

    last_avail_idx: u16,

    desc_paddr: u64,
    desc: *mut VirtQueueDesc,
    avail_paddr: u64,
    avail: *mut VirtQueueAvail,
    used_paddr: u64,
    used: *mut VirtQueueUsed,
}

/* Get location of event indices (only with VIRTIO_F_EVENT_IDX) */
impl VirtQueue {
    pub(crate) fn new(ram_base_raw: *mut u8, queue_num: u32) -> Self {
        Self {
            queue_num,
            ram_base_raw,

            last_avail_idx: 0,

            desc_paddr: 0,
            desc: null_mut::<VirtQueueDesc>(),
            avail_paddr: 0,
            avail: null_mut::<VirtQueueAvail>(),
            used_paddr: 0,
            used: null_mut::<VirtQueueUsed>(),
        }
    }
    pub(crate) fn set_desc_low(&mut self, addr_low: u32) {
        self.desc_paddr &= !(u32::MAX as u64);
        self.desc_paddr |= addr_low as u64;
        self.update_desc_base(self.desc_paddr);
    }
    pub(crate) fn set_desc_high(&mut self, addr_high: u32) {
        self.desc_paddr &= !((u32::MAX as u64) << 32);
        self.desc_paddr |= (addr_high as u64) << 32;
        self.update_desc_base(self.desc_paddr);
    }
    pub(crate) fn set_avail_low(&mut self, addr_low: u32) {
        self.avail_paddr &= !(u32::MAX as u64);
        self.avail_paddr |= addr_low as u64;
        self.update_avail_base(self.avail_paddr);
    }
    pub(crate) fn set_avail_high(&mut self, addr_high: u32) {
        self.avail_paddr &= !((u32::MAX as u64) << 32);
        self.avail_paddr |= (addr_high as u64) << 32;
        self.update_avail_base(self.avail_paddr);
    }
    pub(crate) fn set_used_low(&mut self, addr_low: u32) {
        self.used_paddr &= !(u32::MAX as u64);
        self.used_paddr |= addr_low as u64;
        self.update_used_base(self.used_paddr);
    }
    pub(crate) fn set_used_high(&mut self, addr_high: u32) {
        self.used_paddr &= !((u32::MAX as u64) << 32);
        self.used_paddr |= (addr_high as u64) << 32;
        self.update_used_base(self.used_paddr);
    }

    fn update_avail_base(&mut self, paddr: u64) {
        if paddr >= ram_config::BASE_ADDR {
            self.avail = unsafe {
                self.ram_base_raw
                    .add((paddr - ram_config::BASE_ADDR) as usize)
            } as *mut VirtQueueAvail;
        }
    }
    fn update_desc_base(&mut self, paddr: u64) {
        if paddr >= ram_config::BASE_ADDR {
            self.desc = unsafe {
                self.ram_base_raw
                    .add((paddr - ram_config::BASE_ADDR) as usize)
            } as *mut VirtQueueDesc;
        }
    }
    fn update_used_base(&mut self, paddr: u64) {
        if paddr >= ram_config::BASE_ADDR {
            self.used = unsafe {
                self.ram_base_raw
                    .add((paddr - ram_config::BASE_ADDR) as usize)
            } as *mut VirtQueueUsed;
        }
    }

    pub(super) fn get_used_ring(&self) -> &mut VirtQueueUsed {
        unsafe { self.used.as_mut().unwrap() }
    }

    // unsafe fn get_desc<'a>(
    //     &'a self,
    //     table: *mut VirtQueueDesc,
    //     ram_base: *mut u8,
    //     queue_num: u32,
    //     idx: u16,
    // ) -> VirtQueueDescHandle<'a> {
    //     VirtQueueDescHandle::new(table, ram_base, queue_num, idx as usize)
    // }

    fn try_get_desc(&mut self) -> Option<VirtQueueDescHandle<'_>> {
        let virt_queue_avail = unsafe { self.avail.as_ref().unwrap() };
        virt_queue_avail
            .try_get_desc_idx(self.queue_num, &mut self.last_avail_idx)
            .map(|idx| {
                VirtQueueDescHandle::new(
                    self.desc,
                    self.ram_base_raw as usize,
                    self.queue_num,
                    idx as usize,
                )
            })
    }

    fn insert_used(&mut self, elem: VirtQueueUsedElem) {
        let virt_queue_used = unsafe { self.used.as_mut().unwrap() };
        virt_queue_used.insert_used(self.queue_num, elem);
    }

    /// # [`VirtQueue::manage_one_request<F>(&mut self, func: F)`]
    /// Manage a single request in the virtqueue.
    /// Fn (desc: &VirtQueueDesc) -> u32
    /// Input: The descriptor to manage.
    /// Output: The length of data processed in this descriptor.
    pub(crate) fn manage_one_request<F>(&mut self, mut func: F) -> bool
    where
        F: FnMut(&VirtQueueDesc, usize) -> u32,
    {
        if (self.queue_num == 0)
            || (self.desc.is_null())
            || (self.avail.is_null())
            || (self.used.is_null())
        {
            error!("VirtQueue not ready to manage requests.");
            return false;
        }
        if let Some(mut handle) = self.try_get_desc() {
            let entry_idx = handle.get_entry_idx();
            let mut len = 0;
            let mut idx = 0;

            loop {
                if let Some(desc) = handle.try_get() {
                    len += func(desc, idx);
                    idx += 1;
                } else {
                    break;
                }
            }

            self.get_used_ring()
                .insert_used(self.queue_num, VirtQueueUsedElem { id: entry_idx, len });

            true
        } else {
            false
        }
    }

    pub(crate) fn set_used_ring_flag(&mut self, flag: VirtQueueUsedFlag) {
        self.get_used_ring().flags = flag;
    }

    pub(crate) fn get_avail_flag(&self) -> VirtQueueAvailFlag {
        unsafe { self.avail.as_ref().unwrap().flags }
    }

    pub(super) fn set_queue_num(&mut self, num: u32) {
        self.queue_num = num;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ram;

    #[test]
    fn test_virt_queue_avail_ring() {
        const QUEUE_NUM: usize = 8;
        const DESC_NUM: usize = 8;
        let mut ram = ram::Ram::new();
        let ram_base = &mut ram[0] as *mut u8;
        let mut virt_queue = VirtQueue::new(ram_base, QUEUE_NUM as u32);

        let virtq_desc_base = 0x8000_2000 as u64;
        let virtq_avail_base = 0x8000_2100 + ((QUEUE_NUM + 2) * size_of::<u16>()) as u64;
        let virtq_used_base = 0x8000_2200 + (QUEUE_NUM * size_of::<VirtQueueUsed>() + 4) as u64;
        virt_queue.update_avail_base(virtq_avail_base);
        virt_queue.update_desc_base(virtq_desc_base);
        virt_queue.update_used_base(virtq_used_base);

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
        virtq_avail
            .idx
            .store(0, std::sync::atomic::Ordering::Relaxed);
        let avail_ring = VirtQueueAvail::mut_ring(virtq_avail as *mut _ as u64, QUEUE_NUM as u32);

        // Used Ring.
        let virtq_used = &mut ram[(virtq_used_base - ram_config::BASE_ADDR) as usize] as *mut u8
            as *mut VirtQueueUsed;
        let virtq_used = unsafe { virtq_used.as_mut().unwrap() };
        virtq_used
            .idx
            .store(0, std::sync::atomic::Ordering::Relaxed);
        virtq_used.flags = VirtQueueUsedFlag::Default;
        let _used_ring = virtq_used.ring(QUEUE_NUM as u32);

        // test Less-End.
        virtq_avail.flags = VirtQueueAvailFlag::NoInterrupt;
        assert_eq!(
            ram[(virtq_avail_base - ram_config::BASE_ADDR) as usize],
            0x1
        );

        // Write Available Ring.
        avail_ring[0] = 0;

        let desc0 = &mut virt_queue_desc[0];
        desc0.paddr = 0x8000_2300;
        desc0.len = 0x10;
        desc0.flags = VirtQueueDescFlag::VIRTQ_DESC_F_NEXT;
        desc0.next = 1;
        ram.write(desc0.paddr - ram_config::BASE_ADDR, 114514usize)
            .unwrap();

        let desc1 = &mut virt_queue_desc[1];
        desc1.paddr = 0x8000_2310;
        desc1.len = 0x10;
        desc1.flags = VirtQueueDescFlag::VIRTQ_DESC_F_NEXT;
        desc1.next = 2;
        ram.write(desc1.paddr - ram_config::BASE_ADDR, 0721usize)
            .unwrap();

        let desc2 = &mut virt_queue_desc[2];
        desc2.paddr = 0x8000_2320;
        desc2.len = 0x10;
        desc2.flags = VirtQueueDescFlag::empty();
        desc2.next = 3;
        ram.write(desc2.paddr - ram_config::BASE_ADDR, 998244353usize)
            .unwrap();

        // Test getting descriptors.
        virt_queue.try_get_desc().map(|mut handle| {
            let desc0_result = handle.try_get().unwrap();
            assert_eq!(desc0_result.paddr, 0x8000_2300);
            assert_eq!(desc0_result.len, 0x10);
            assert!(
                desc0_result
                    .flags
                    .contains(VirtQueueDescFlag::VIRTQ_DESC_F_NEXT)
            );
            assert_eq!(desc0_result.next, 1);
            let buf0: &[usize] = unsafe {
                slice::from_raw_parts_mut(
                    desc0_result.get_request_package(ram_base as usize),
                    desc0_result.len as usize / std::mem::size_of::<usize>(),
                )
            };
            assert_eq!(buf0[0], 114514);

            let desc1_result = handle.try_get().unwrap();
            assert_eq!(desc1_result.paddr, 0x8000_2310);
            assert_eq!(desc1_result.len, 0x10);
            assert!(
                desc1_result
                    .flags
                    .contains(VirtQueueDescFlag::VIRTQ_DESC_F_NEXT)
            );
            assert_eq!(desc1_result.next, 2);
            let buf1: &[usize] = unsafe {
                slice::from_raw_parts_mut(
                    desc1_result.get_request_package(ram_base as usize),
                    desc1_result.len as usize / std::mem::size_of::<usize>(),
                )
            };
            assert_eq!(buf1[0], 0721);

            let desc2_result = handle.try_get().unwrap();
            assert_eq!(desc2_result.paddr, 0x8000_2320);
            assert_eq!(desc2_result.len, 0x10);
            assert!(desc2_result.flags.contains(VirtQueueDescFlag::empty()));
            assert_eq!(desc2_result.next, 3);
            let buf2: &[usize] = unsafe {
                slice::from_raw_parts_mut(
                    desc2_result.get_request_package(ram_base as usize),
                    desc2_result.len as usize / std::mem::size_of::<usize>(),
                )
            };
            assert_eq!(buf2[0], 998244353);

            assert!(handle.try_get().is_none());
        });
    }

    #[test]
    fn test_virt_queue_used_ring() {
        const QUEUE_NUM: usize = 8;
        const DESC_NUM: usize = 8;
        let mut ram = ram::Ram::new();
        let ram_base = &mut ram[0] as *mut u8;
        let mut virt_queue = VirtQueue::new(ram_base, QUEUE_NUM as u32);

        let virtq_desc_base = 0x8000_2000 as u64;
        let virtq_avail_base = 0x8000_2100 + ((QUEUE_NUM + 2) * size_of::<u16>()) as u64;
        let virtq_used_base = 0x8000_2200 + (QUEUE_NUM * size_of::<VirtQueueUsed>() + 4) as u64;
        virt_queue.set_avail_low(virtq_desc_base as u32);
        virt_queue.set_avail_high((virtq_desc_base >> 32) as u32);
        virt_queue.set_desc_low(virtq_desc_base as u32);
        virt_queue.set_desc_low((virtq_desc_base >> 32) as u32);
        virt_queue.set_used_low(virtq_used_base as u32);
        virt_queue.set_used_high((virtq_used_base >> 32) as u32);
        // virt_queue.update_avail_base(virtq_avail_base);
        // virt_queue.update_desc_base(virtq_desc_base);
        // virt_queue.update_used_base(virtq_used_base);

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
        virtq_avail
            .idx
            .store(0, std::sync::atomic::Ordering::Relaxed);
        let avail_ring = VirtQueueAvail::mut_ring(virtq_avail as *mut _ as u64, QUEUE_NUM as u32);

        // Used Ring.
        let virtq_used = &mut ram[(virtq_used_base - ram_config::BASE_ADDR) as usize] as *mut u8
            as *mut VirtQueueUsed;
        let virtq_used = unsafe { virtq_used.as_mut().unwrap() };
        virtq_used
            .idx
            .store(0, std::sync::atomic::Ordering::Relaxed);
        virtq_used.flags = VirtQueueUsedFlag::Default;
        let _used_ring = virtq_used.ring(QUEUE_NUM as u32);

        // Write Available Ring.
        avail_ring[0] = 0;

        let desc0 = &mut virt_queue_desc[0];
        desc0.paddr = 0x8000_2300;
        desc0.len = 0x10;
        desc0.flags = VirtQueueDescFlag::empty();
        desc0.next = 1;
        ram.write(desc0.paddr - ram_config::BASE_ADDR, 114514u32)
            .unwrap();

        // Test getting descriptors.
        virt_queue.manage_one_request(|desc, _| {
            let buf: &mut [u32] = unsafe {
                slice::from_raw_parts_mut(
                    desc.get_request_package(ram_base as usize),
                    desc.len as usize / std::mem::size_of::<u32>(),
                )
            };
            assert_eq!(buf[0], 114514);
            assert_eq!(buf.len(), 0x10 / size_of::<u32>());

            // Write Used Ring.
            buf[0] = 0x0123;
            buf[1] = 0x4567;
            buf[2] = 0x89ab;
            buf[3] = 0xcdef;
            0x10
        });

        assert_eq!(virtq_used.idx.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert_eq!(
            ram.read::<u32>(0x8000_2300 - ram_config::BASE_ADDR)
                .unwrap(),
            0x0123
        );
        assert_eq!(
            ram.read::<u32>(0x8000_2304 - ram_config::BASE_ADDR)
                .unwrap(),
            0x4567
        );
        assert_eq!(
            ram.read::<u32>(0x8000_2308 - ram_config::BASE_ADDR)
                .unwrap(),
            0x89ab
        );
        assert_eq!(
            ram.read::<u32>(0x8000_230c - ram_config::BASE_ADDR)
                .unwrap(),
            0xcdef
        );
    }
}
