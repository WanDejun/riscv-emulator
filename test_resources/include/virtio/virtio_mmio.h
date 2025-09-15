#pragma once
#include <stdatomic.h>
#include <stdint.h>

enum VirtIODeviceStatus: uint8_t {
    /// Device acknowledges that it has seen the device and knows what it is.
    ACKNOWLEDGE = 1 << 0,
    /// Driver has found a usable device.
    DRIVER = 1 << 1,
    /// Driver has set up the device.
    DRIVER_OK = 1 << 2,
    /// Driver has failed to set up the device or device has encountered an error.
    FAILED = 1 << 7,
    /// Indicates that the driver is set up and ready to drive the device
    FEATURES_OK = 1 << 3,
    /// Device needs to be reset.
    DEVICE_NEEDS_RESET = 1 << 6,
};

enum VirtIODeviceInterrupt: uint32_t {
    /// The device is making a request for the driver to do something
    VIRTIO_MMIO_INT_VRING = 1 << 0,
    /// The device has an error to report to the driver
    VIRTIO_MMIO_INT_CONFIG = 1 << 1,
};

typedef struct VirtIOMMIOQueueStatus {
    uint64_t desc;
    uint64_t avail;
    uint64_t used;
    atomic_bool enable;
} VirtIOMMIOQueueStatus;
