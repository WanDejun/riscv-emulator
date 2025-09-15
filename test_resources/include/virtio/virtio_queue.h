#pragma once
#include <stdatomic.h>
#include <stdint.h>

// =====================================
//           VirtQueueDesc
// =====================================
enum VirtQueueDescFlag: uint16_t {
    /* This marks a buffer as continuing via the next field. */
    VIRTQ_DESC_F_NEXT     = 1 << 0,

    /* This marks a buffer as device write-only (otherwise device read-only). */
    VIRTQ_DESC_F_WRITE    = 1 << 1,

    /* This means the buffer contains a list of buffer descriptors. */
    VIRTQ_DESC_F_INDIRECT = 1 << 2,
};

typedef struct VirtQueueDesc {
    /* Address (guest-physical). */
    uint64_t paddr;
    /* Length. */
    uint32_t len;
    /* The flags as indicated above. */
    enum VirtQueueDescFlag flags;
    /* Next field if flags & NEXT */
    uint16_t next;
} VirtQueueDesc;


// =====================================
//           VirtQueueAvail
// =====================================
enum VirtQueueAvailFlag: uint16_t {
    AvailFlagDefault = 0,     // Interrupt the device when a `UsedRing` is consumed.
    AvailFlagNoInterrupt = 1, // Driver will polling the `UsedRing`. Do not need to interrupt the device.
};

typedef struct  {
    enum VirtQueueAvailFlag flags;  // Written by Driver. (u16)
    // TODO!
    // atomic_char16_t idx;            // Written by Driver.
    uint16_t idx;
    uint16_t ring[];
    /* ring1 ... */
    /* Only if VIRTIO_F_EVENT_IDX: used_event: u16; */
} VirtQueueAvail;

// =====================================
//           VirtQueueUsed
// =====================================
enum VirtQueueUsedFlag: uint16_t {
    UsedFlagDefault = 0,  // Notify the device when a `AvailRing` is used.
    UsedFlagNoNotify = 1, // Device will polling the `AvailRing`. Do not notify the device.
};

/* le32 is used here for ids for padding reasons. */
typedef struct  {
    /* Index of start of used descriptor chain. */
    uint32_t id;
    /* Total length of the descriptor chain which was written to. */
    uint32_t len;
} VirtQueueUsedElem;

typedef struct  {
    enum VirtQueueUsedFlag flags; // Written by Device. (u16)
    atomic_char16_t idx;          // Written by Device.
    VirtQueueUsedElem ring[];
    /* ring1 ... */
    /* Only if VIRTIO_F_EVENT_IDX: avail_event: u16; */
} VirtQueueUsed;

