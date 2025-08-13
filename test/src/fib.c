#include "io.h"
#include <stdint.h>

// 定义 UART 内存映射地址 (QEMU virt 平台)
#define UART_ADDR 0x10000000
#define UART_LSR_ADDR (UART_ADDR + 0x05)
#define UART_LSR_THRE 0x20 // Bit 5: Transmitter Holding Register Empty
#define VIRT_POWEROFF_ADDR 0x100000

const int N = 6;

void output(int x) {
    if (x) {
        output(x / 10);
        uart_putc(x % 10 + '0');
    }
}

int fib(int n) {
    if (n == 1 || n == 2) {
        return 1;
    }

    return fib(n - 1) + fib(n - 2);
}

int main() {
    output(fib(8));
    uart_putc('\n');

    // while (1) {}
    return 0;
}