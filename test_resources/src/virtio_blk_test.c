#include "io.h"
#include "power.h"
#include "virtio/virtio_mmio.h"
#include "virtio/virtio_queue.h"
#include "virtio/virtio_blk.h"
#include "log.h"
#include <stdint.h>

#define QUEUE_SIZE 8
#define AVAIL_RING_SIZE (sizeof(uint16_t) * QUEUE_SIZE + sizeof(VirtQueueAvail))
#define USED_RING_SIZE (sizeof(VirtQueueUsedElem) * QUEUE_SIZE + sizeof(VirtQueueUsed))
#define DESC_SIZE 16
#define BLOCK_SIZE 512

struct VirtIOMMIOLayout {
    uint32_t magic_value;           // 0x000
    uint32_t version;               // 0x004
    uint32_t device_id;             // 0x008
    uint32_t vendor_id;             // 0x00c
    uint32_t device_features;       // 0x010
    uint32_t device_features_sel;   // 0x014
    uint32_t reserved_0[2];         // 0x018, 0x01c
    uint32_t driver_features;       // 0x020
    uint32_t driver_features_sel;   // 0x024
    uint32_t reserved_1[2];         // 0x028, 0x02c
    uint32_t queue_sel;             // 0x030
    uint32_t queue_num_max;         // 0x034
    uint32_t queue_num;             // 0x038
    uint32_t queue_align;           // 0x03c
    uint32_t queue_pfn;             // 0x040
    uint32_t queue_ready;           // 0x044
    uint32_t reserved_2[2];         // 0x048, 0x04c
    uint32_t queue_notify;          // 0x050
    uint32_t reserved_3[3];         // 0x054, 0x058, 0x05c
    uint32_t interrupt_status;      // 0x060
    uint32_t interrupt_ack;         // 0x064
    uint32_t reserved_4[2];         // 0x068, 0x06c
    uint32_t status;                // 0x070
    uint32_t reserved_5[3];         // 0x074, 0x078, 0x07c
    uint32_t queue_desc_low;        // 0x080
    uint32_t queue_desc_high;       // 0x084
    uint32_t reserved_6[2];         // 0x088, 0x08c
    uint32_t queue_avail_low;       // 0x090
    uint32_t queue_avail_high;      // 0x094
    uint32_t reserved_7[2];         // 0x098, 0x09c
    uint32_t queue_used_low;        // 0x0A0
    uint32_t queue_used_high;       // 0x0A4
    uint32_t reserved_8[22];        // 22 words, 0x0A8 ~ 0x0FC
    uint8_t  config_space[];        // 0x100
};

// 直接指向 MMIO 设备
#define VIRTIO_MMIO_BASE 0x10001000
volatile struct VirtIOMMIOLayout *virtio_blk1 = (volatile struct VirtIOMMIOLayout *)VIRTIO_MMIO_BASE;

uint8_t buf[BLOCK_SIZE][8] __attribute__((aligned(4096)));
VirtQueueDesc desc[DESC_SIZE * BLOCK_SIZE];
uint8_t avail_base[AVAIL_RING_SIZE];
VirtQueueAvail* avail = (VirtQueueAvail*)avail_base;
uint8_t used_base[USED_RING_SIZE];
VirtQueueUsed* used = (VirtQueueUsed*)used_base;

int main() {
    TEST_START(__func__);

    // print count
    virtio_blk1->status = ACKNOWLEDGE;
    virtio_blk1->status = ACKNOWLEDGE | DRIVER;

    virtio_blk1->device_features_sel = 0;
    uint64_t host_features = virtio_blk1->device_features;
    virtio_blk1->device_features_sel = 1;
    host_features |= ((uint64_t)virtio_blk1->device_features) << 32;

    virtio_blk1->driver_features_sel = 0;
    uint64_t guest_features = host_features;
    virtio_blk1->driver_features = (uint32_t)(guest_features & 0xffffffff);
    virtio_blk1->driver_features_sel = 0;
    virtio_blk1->driver_features = (uint32_t)(guest_features >> 32);

    virtio_blk1->status = ACKNOWLEDGE | DRIVER | FEATURES_OK;
    if (!(virtio_blk1->status & FEATURES_OK)) {
        // feature negotiation failed
        printf("Feature negotiation failed\n");
        PowerOff();
    }

    // setup queue
    virtio_blk1->queue_sel = 0;
    if (virtio_blk1->queue_num_max == 0) {
        // no queue 0
        printf("No queue 0\n");
        PowerOff();
    }
    virtio_blk1->queue_num = QUEUE_SIZE;

    virtio_blk1->queue_desc_low = (uint64_t)(desc) & 0xffffffff;
    virtio_blk1->queue_desc_high = ((uint64_t)(desc) >> 32) & 0xffffffff;
    virtio_blk1->queue_avail_low = (uint64_t)(avail) & 0xffffffff;
    virtio_blk1->queue_avail_high = ((uint64_t)(avail) >> 32) & 0xffffffff;
    virtio_blk1->queue_used_low = (uint64_t)(used) & 0xffffffff;
    virtio_blk1->queue_used_high = ((uint64_t)(used) >> 32) & 0xffffffff;

    virtio_blk1->queue_ready = 1;

    avail->flags = 0;
    avail->idx = 0;
    used->flags = 0;
    used->idx = 0;

    // head
    VirtQueueDesc* desc0 = (&desc[0]);
    VirtioBlkReq req = {
        .request_type = VirtioBlkReqOut, // VIRTIO_BLK_T_IN
        .reserved = 0,
        .sector = 0,
    };
    desc0->paddr = (uint64_t)(&req);
    desc0->len = sizeof(VirtioBlkReq);
    desc0->flags = VIRTQ_DESC_F_NEXT;
    desc0->next = 1;

    // body
    for (int i = 0; i < BLOCK_SIZE; i++) {
        buf[0][i] = i;
    }
    VirtQueueDesc* desc1 = (&desc[1]);
    desc1->paddr = (uint64_t)(buf[0]);
    desc1->len = BLOCK_SIZE;
    desc1->flags = VIRTQ_DESC_F_NEXT;
    desc1->next = 2;

    // tail
    uint8_t status = 111;
    VirtQueueDesc* desc2 = (&desc[2]);
    desc2->paddr = (uint64_t)(&status); // status byte
    desc2->len = 1;
    desc2->flags = VIRTQ_DESC_F_WRITE;
    desc2->next = 0;

    avail->ring[0] = 0; // desc index
    avail->idx += 1;

    virtio_blk1->queue_notify = 0;
    // TODO!
    // if enable interrupt, should wait interrupt here.

    if (status != 0 /*OK*/) {
        Log(ERROR, "First read failed: %d\n", status);
        PowerOff();
    }

    /// ========================================

    // head
    desc0 = (&desc[0]);
    req.request_type = VirtioBlkReqIn; // VIRTIO_BLK_T_IN
    desc0->paddr = (uint64_t)(&req);
    desc0->len = sizeof(VirtioBlkReq);
    desc0->flags = VIRTQ_DESC_F_NEXT;
    desc0->next = 1;

    // body
    for (int i = 0; i < BLOCK_SIZE; i++) {
        buf[0][i] = 0;
    }
    desc1 = (&desc[1]);
    desc1->paddr = (uint64_t)(buf[0]);
    desc1->len = BLOCK_SIZE;
    desc1->flags = VIRTQ_DESC_F_NEXT | VIRTQ_DESC_F_WRITE;
    desc1->next = 2;

    // tail
    status = 0;
    desc2 = (&desc[2]);
    desc2->paddr = (uint64_t)(&status); // status byte
    desc2->len = 1;
    desc2->flags = VIRTQ_DESC_F_WRITE;
    desc2->next = 0;

    avail->ring[0] = 0; // desc index
    avail->idx += 1;

    virtio_blk1->queue_notify = 0;

    for (int i = 0; i < BLOCK_SIZE; i++) {
        // printf("%4d", buf[0][i]);
        if (buf[0][i] != (i & 0xff)) {
            Log(ERROR, "Read data error: buf[%d] = %d\n", i, buf[i]);
            PowerOff();
        }
    }

    TEST_END(__func__);
    PASS;
    PowerOff();
    return 0;
}
