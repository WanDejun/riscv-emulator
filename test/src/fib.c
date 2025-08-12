#include "io.h"
#include <stdint.h>

// 定义 UART 内存映射地址 (QEMU virt 平台)
#define UART_ADDR 0x10000000
#define UART_LSR_ADDR (UART_ADDR + 0x05)
#define UART_LSR_THRE 0x20 // Bit 5: Transmitter Holding Register Empty
#define VIRT_POWEROFF_ADDR 0x100000

const int N = 3;

void output(int x) {
    if (x) {
        output(x / 10);
        uart_putc(x % 10 + '0');
    }
}

int main() {
    int a = 1, b = 1, c;
    for (int i = 1; i <= N; i++) {
        c = a + b;
        a = b;
        b = c;
    }

    output(c);

    // uart_putc('\n');
    // volatile uint32_t *poweroff = (uint32_t *)VIRT_POWEROFF_ADDR;
    // *poweroff = 0x5555;
    // while (1) { /* 等待 QEMU 退出 */
    // }

    return 0;
}