#include "io.h"
#define VIRT_POWEROFF_ADDR 0x100000

void PowerOff() {
    uart_putc('\n');
    volatile uint32_t* poweroff = (uint32_t*)VIRT_POWEROFF_ADDR;
    *poweroff = 0x5555;
    while (1) { /* 等待 QEMU 退出 */
    }
}