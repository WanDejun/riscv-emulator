use core::slice;
use std::sync::atomic::AtomicU16;

use bitflags::bitflags;

use crate::ram_config;

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
    len: u32,
    /* The flags as indicated above. */
    flags: VirtQueueDescFlag,
    /* Next field if flags & NEXT */
    next: u16,
}

impl VirtQueueDesc {
    pub(crate) fn get_buffer<T>(&self, ram_base_raw: usize) -> &mut [T] {
        unsafe {
            std::slice::from_raw_parts_mut(
                (self.paddr - ram_config::BASE_ADDR + ram_base_raw as u64) as *mut T,
                self.len as usize / std::mem::size_of::<T>(),
            )
        }
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

#[repr(C)]
// (6 + QueueNum * 2) bytes
pub(crate) struct VirtQueueAvail {
    flags: u16,     // defined by device.
    idx: AtomicU16, // Written by Driver.
    ring0: u16,
    /* ring1 ... */
    /* Only if VIRTIO_F_EVENT_IDX: used_event: u16; */
}

impl VirtQueueAvail {
    fn ring(&self, queue_num: u32) -> &[u16] {
        unsafe { std::slice::from_raw_parts(&self.ring0 as *const u16, queue_num as usize) }
    }

    #[cfg(test)]
    fn mut_ring(base: u64, queue_num: u32) -> &'static mut [u16] {
        unsafe { std::slice::from_raw_parts_mut((base + 4) as *mut u16, queue_num as usize) }
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

/* le32 is used here for ids for padding reasons. */
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct VirtqUsedElem {
    /* Index of start of used descriptor chain. */
    id: u32,
    /* Total length of the descriptor chain which was written to. */
    len: u32,
}

#[repr(C)]
// (6 + QueueNum * 8) bytes
pub(crate) struct VirtQueueUsed {
    flags: u16,
    idx: AtomicU16, // Written by Device. (Locked in VirtQueue).
    ring0: VirtqUsedElem,
    /* ring1 ... */
    /* Only if VIRTIO_F_EVENT_IDX: used_event: u16; */
}

impl VirtQueueUsed {
    fn ring(&mut self, queue_num: u32) -> &mut [VirtqUsedElem] {
        unsafe {
            std::slice::from_raw_parts_mut(
                &mut self.ring0 as *mut VirtqUsedElem,
                queue_num as usize,
            )
        }
    }

    fn insert_used(&mut self, queue_num: u32, elem: VirtqUsedElem) {
        let mut idx = self.idx.load(std::sync::atomic::Ordering::Relaxed);
        idx += 1;
        idx %= queue_num as u16;
        self.idx.store(idx, std::sync::atomic::Ordering::Relaxed);

        self.ring(queue_num)[idx as usize] = elem;
    }
}

/// Needs to be wrapped in a Mutex.
pub(crate) struct VirtQueue {
    queue_num: u32,
    ram_base_raw: *mut u8,

    last_avail_idx: u16,

    desc: *mut VirtQueueDesc,
    avail: *mut VirtQueueAvail,
    used: *mut VirtQueueUsed,
}

/* Get location of event indices (only with VIRTIO_F_EVENT_IDX) */
impl VirtQueue {
    pub(crate) fn new(ram_base: *mut u8, queue_num: u32) -> Self {
        Self {
            queue_num,
            ram_base_raw: ram_base,

            last_avail_idx: 0,

            desc: ram_base as *mut VirtQueueDesc,
            avail: ram_base as *mut VirtQueueAvail,
            used: ram_base as *mut VirtQueueUsed,
        }
    }

    pub(crate) fn update_avail(&mut self, paddr: u64) {
        self.avail = unsafe {
            self.ram_base_raw
                .add((paddr - ram_config::BASE_ADDR) as usize)
        } as *mut VirtQueueAvail;
    }
    pub(crate) fn update_desc(&mut self, paddr: u64) {
        self.desc = unsafe {
            self.ram_base_raw
                .add((paddr - ram_config::BASE_ADDR) as usize)
        } as *mut VirtQueueDesc;
    }
    pub(crate) fn update_used(&mut self, paddr: u64) {
        self.used = unsafe {
            self.ram_base_raw
                .add((paddr - ram_config::BASE_ADDR) as usize)
        } as *mut VirtQueueUsed;
    }

    fn get_used_ring(&self) -> &mut VirtQueueUsed {
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

    fn insert_used(&mut self, elem: VirtqUsedElem) {
        let virt_queue_used = unsafe { self.used.as_mut().unwrap() };
        virt_queue_used.insert_used(self.queue_num, elem);
    }

    /// # [`VirtQueue::manage_one_request<F>(&mut self, func: F)`]
    /// Manage a single request in the virtqueue.
    /// Fn (desc: &VirtQueueDesc) -> u32
    /// Input: The descriptor to manage.
    /// Output: The length of data processed in this descriptor.
    pub(crate) fn manage_one_request<F>(&mut self, func: F)
    where
        F: Fn(&VirtQueueDesc) -> u32,
    {
        if let Some(mut handle) = self.try_get_desc() {
            let entry_idx = handle.get_entry_idx();
            let mut len = 0;

            loop {
                if let Some(desc) = handle.try_get() {
                    len += func(desc);
                } else {
                    break;
                }
            }

            self.get_used_ring()
                .insert_used(self.queue_num, VirtqUsedElem { id: entry_idx, len });
        }
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
        virt_queue.update_avail(virtq_avail_base);
        virt_queue.update_desc(virtq_desc_base);
        virt_queue.update_used(virtq_used_base);

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
        virtq_used.flags = 0;
        let _used_ring = virtq_used.ring(QUEUE_NUM as u32);

        // test Less-End.
        virtq_avail.flags = 0xab_cd;
        assert_eq!(
            ram[(virtq_avail_base - ram_config::BASE_ADDR) as usize],
            0xcd
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
            let buf0: &[usize] = desc0_result.get_buffer(ram_base as usize);
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
            let buf1: &[usize] = desc1_result.get_buffer(ram_base as usize);
            assert_eq!(buf1[0], 0721);

            let desc2_result = handle.try_get().unwrap();
            assert_eq!(desc2_result.paddr, 0x8000_2320);
            assert_eq!(desc2_result.len, 0x10);
            assert!(desc2_result.flags.contains(VirtQueueDescFlag::empty()));
            assert_eq!(desc2_result.next, 3);
            let buf2: &[usize] = desc2_result.get_buffer(ram_base as usize);
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
        virt_queue.update_avail(virtq_avail_base);
        virt_queue.update_desc(virtq_desc_base);
        virt_queue.update_used(virtq_used_base);

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
        virtq_used.flags = 0;
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
        virt_queue.manage_one_request(|desc| {
            let buf: &mut [u32] = desc.get_buffer(ram_base as usize);
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
