#include "io.h"
#include <stdint.h>

#define VIRT_POWEROFF_ADDR 0x100000

void PowerOff() {
    uart_putc('\n');
    volatile uint32_t* poweroff = (uint32_t*)VIRT_POWEROFF_ADDR;
    *poweroff = 0x5555;
    while (1) { }
}

int main() {
    for (int i = 0; i < 500; ++i) {
        uart_putc('A');
    }

    for (int i = 0; i < 500; ++i) {
        printf("%d", i);
    }

    uart_putc('\n');
    PowerOff();
    return 0;
}
