#pragma once
#include <stdint.h>

enum VirtioBlkReqType:uint32_t {
    VirtioBlkReqIn = 0,
    VirtioBlkReqOut = 1,
    VirtioBlkReqFlush = 4,
    VirtioBlkReqGetId = 8,
    VirtioBlkReqGetLifetime = 10,
    VirtioBlkReqDiscard = 11,
    VirtioBlkReqWriteZeroes = 13,
    VirtioBlkReqSecureErase = 14,
    VirtioBlkReqUnsupported = 0xFFFFFFFF,
};

typedef struct VirtioBlkReq {
    enum VirtioBlkReqType request_type; // (VirtioBlkReqStatus)
    uint32_t reserved;
    uint64_t sector;
} VirtioBlkReq;
