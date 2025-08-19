#include "io.h"
#include <stdint.h>

#define VIRT_POWEROFF_ADDR 0x100000

void PowerOff() {
    uart_putc('\n');
    volatile uint32_t* poweroff = (uint32_t*)VIRT_POWEROFF_ADDR;
    *poweroff = 0x5555;
    while (1) { }
}

int is_prime(int n) {
    if (n <= 1) return 0;
    for (int i = 2; i * i <= n; ++i) {
        if (n % i == 0) return 0;
    }
    return 1;
}

int main() {
    int count = 0;
    for (int i = 2; i < 20000; ++i) {
        if (is_prime(i)) count++;
    }
    // print count
    printf("%d", count);

    PowerOff();
    return 0;
}
