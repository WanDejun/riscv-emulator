#include "io.h"
#include "log.h"
#include "power.h"
#include "trap.h"

const int XLEN = 64;

const uint64_t CLINT_BASE = 0x2000000;
const uint64_t MTIME_OFFSET = 0xbff8;
const uint64_t MTIMECMP_OFFSET = 0x4000;

uint64_t bit_at(int pos) {
    return 1ull << pos;
}

static volatile unsigned char success = 0;

void trap_handler(TrapContext* trap_ctx) {
    // Machine timer interrupt
    if (read_csr_mcause() == (bit_at(XLEN - 1) | 7)) {
        Log(INFO, "Machine timer interrupt!");
        write_csr_mip(0);
        success = 1;
    }

    __traps_return(trap_ctx);
}

uint64_t read_u64_volatile(uint64_t addr) {
    return *(volatile uint64_t*)addr;
}   

uint64_t write_u64_volatile(uint64_t addr, uint64_t value) {
    *(volatile uint64_t*)addr = value;
    return value;
}

int main() {
    TEST_START(__BASE_FILE__);
    trap_init();

    uint64_t mtime = read_u64_volatile(CLINT_BASE + MTIME_OFFSET);
    write_u64_volatile(CLINT_BASE + MTIMECMP_OFFSET, mtime + 4096);

    Log(DEBUG, "waiting for timer interrupt...");
    while (success == 0) {
        // do nothing
    }

    TEST_END(__BASE_FILE__);

    PowerOff();
    return 0;
}