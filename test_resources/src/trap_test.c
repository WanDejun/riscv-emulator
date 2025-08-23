#include "io.h"
#include "trap.h"
#include <stdint.h>

int trap_values[10];
int standard[10] = { 5, 7, 4, 6 };
int trap_cnt;

void trap_handler(TrapContext* trap_ctx) {
    trap_values[trap_cnt] = read_csr_mcause();
    printf("mcause: %x\n", trap_values[trap_cnt]);
    trap_cnt++;
    trap_ctx->mepc += 4;
    printf("mtval: %x\n", read_csr_mtval());
    __traps_return(trap_ctx);
}

int main() {
    TEST_START(__BASE_FILE__);

    trap_init();
    uint64_t* illigal_ptr = (uint64_t*)(0x11110000);

    uint64_t val = *illigal_ptr;  // Load Fault (5)

    *illigal_ptr = 4;  // Store Fault (7)

    illigal_ptr = (uint64_t*)(0x11110001);
    val = *illigal_ptr;  // Load Misaligned (4)

    *illigal_ptr = 5;  // Store Misaligned (6)

    if (trap_cnt != 4)
        FAIL;
    for (int i = 0; i < 4; i++) {
        if (trap_values[i] != standard[i])
            FAIL;
    }

    TEST_START(__BASE_FILE__);
    PASS;

    return 0;
}