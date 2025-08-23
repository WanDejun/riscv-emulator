#include "io.h"
#include "power.h"
#include <stdint.h>

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
