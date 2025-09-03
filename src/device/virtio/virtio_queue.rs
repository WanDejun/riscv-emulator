use bitflags::bitflags;

bitflags! {
    pub(crate) struct VirtQueueFlag: u16 {
    /* This marks a buffer as continuing via the next field. */
    const VIRTQ_DESC_F_NEXT     = 1 << 0;

    /* This marks a buffer as device write-only (otherwise device read-only). */
    const VIRTQ_DESC_F_WRITE    = 1 << 1;

    /* This means the buffer contains a list of buffer descriptors. */
    const VIRTQ_DESC_F_INDIRECT = 1 << 2;
    }
}

#[repr(C)]
pub(crate) struct VirtQueueDesc {
    /* Address (guest-physical). */
    addr: u64,
    /* Length. */
    len: u32,
    /* The flags as indicated above. */
    flags: VirtQueueFlag,
    /* Next field if flags & NEXT */
    next: u16,
}

pub(crate) struct VirtQueueAvail {
    flags: u16,
    idx: u16, // Written by Driver.
    ring0: u16,
    /* Only if VIRTIO_F_EVENT_IDX: le16 used_event; */
    /* ring1 ... */
}

impl VirtQueueAvail {
    fn ring(&mut self) -> &mut [u16] {
        unsafe {
            std::slice::from_raw_parts_mut(
                &mut self.ring0 as *mut u16,
                (self.ring0 as usize + 1) * std::mem::size_of::<u16>(),
            )
        }
    }
}

/* le32 is used here for ids for padding reasons. */
pub(crate) struct VirtqUsedElem {
    /* Index of start of used descriptor chain. */
    id: u32,
    /* Total length of the descriptor chain which was written to. */
    len: u32,
}

pub(crate) struct VirtQueueUsed {
    flags: u16,
    idx: u16, // Written by Device.
    ring0: VirtqUsedElem,
    /* Only if VIRTIO_F_EVENT_IDX: le16 avail_event; */
    /* ring1 ... */
}

impl VirtQueueUsed {
    fn ring(&mut self) -> &mut [VirtqUsedElem] {
        unsafe {
            std::slice::from_raw_parts_mut(
                &mut self.ring0 as *mut VirtqUsedElem,
                (self.ring0.id as usize + 1) * std::mem::size_of::<VirtqUsedElem>(),
            )
        }
    }
}

pub(crate) struct VirtQueue {
    num: u32,
    ram_base_raw: *mut u8,

    desc: *mut VirtQueueDesc,
    avail: *mut VirtQueueAvail,
    last_avail_idx: u16,
    used: *mut VirtQueueUsed,
}

/* Get location of event indices (only with VIRTIO_F_EVENT_IDX) */
impl VirtQueue {
    pub fn new(ram_base: *mut u8, queue_size: u32) -> Self {
        Self {
            num: queue_size,
            ram_base_raw: ram_base,

            desc: ram_base as *mut VirtQueueDesc,
            avail: ram_base as *mut VirtQueueAvail,
            last_avail_idx: 0,
            used: ram_base as *mut VirtQueueUsed,
        }
    }

    fn virtq_need_event(event_idx: u16, new_idx: u16, old_idx: u16) -> bool {
        (new_idx - event_idx - 1) < (new_idx - old_idx)
    }

    pub fn update_avail(&mut self, paddr: u64) {
        self.avail = unsafe { self.ram_base_raw.add(paddr as usize) } as *mut VirtQueueAvail;
    }

    pub fn virtq_used_event(&mut self) -> *mut u16 {
        /* For backwards compat, used event index is at *end* of avail ring. */
        &mut unsafe { self.avail.read() }.ring()[self.num as usize] as *mut u16
        // &mut self.avail.ring[self.num as usize] as *mut u16
    }

    pub fn virtq_avail_event(&mut self) -> *mut u16 {
        /* For backwards compat, avail event index is at *end* of used ring. */
        &mut unsafe { self.used.read() }.ring()[self.num as usize] as *mut VirtqUsedElem as *mut u16
    }
}
