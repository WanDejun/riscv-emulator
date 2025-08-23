#include <stdint.h>
#include <trap.h>

int main() {
    trap_init();
    uint64_t* illigal_ptr = (uint64_t*)(0x11110000);
    uint64_t val = *illigal_ptr;

    return 0;
}