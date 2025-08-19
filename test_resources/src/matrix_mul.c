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
    const int N = 64;
    static int A[64][64];
    static int B[64][64];
    static int C[64][64];

    for (int i = 0; i < N; ++i) for (int j = 0; j < N; ++j) { A[i][j] = i + j; B[i][j] = i - j; C[i][j] = 0; }

    for (int i = 0; i < N; ++i) {
        for (int k = 0; k < N; ++k) {
            int tmp = A[i][k];
            for (int j = 0; j < N; ++j) {
                C[i][j] += tmp * B[k][j];
            }
        }
    }

    // print some checksum
    unsigned long sum = 0;
    for (int i = 0; i < N; ++i) for (int j = 0; j < N; ++j) sum += (unsigned long)C[i][j];

    printf("%ld\n", sum);
    
    PowerOff();
    return 0;
}
