#include "trap.h"
#include "io.h"
#include <stdint.h>

typedef struct SAMPLE_TIMER_T {
    uint32_t control;
    uint32_t interrupt_mask;
    uint32_t interval_low;
    uint32_t interval_high;
} SampleTimer;
const uint64_t SAMPLE_TIMER_BASE_ADDR = 0x101000;
const uint32_t SAMPLE_TIMER_CONTROL_RESET = 1u << 0;
volatile SampleTimer* sample_timer = (SampleTimer*)SAMPLE_TIMER_BASE_ADDR;

struct PLICContextConfig {
    volatile uint32_t threshold;
    volatile uint32_t claimed_id;
    uint32_t reserved[0x1000 / 4 - 2];
};
typedef struct PLIC_T {
    uint32_t priority[1024];
    uint32_t pending_bit[1024 / 32];

    uint32_t reversed0[0x3e0];
    
    uint32_t context_enable_bits[15872][1024 / 32];
    uint32_t reversed1[0x3800];

    struct PLICContextConfig context_config[15872];
} PLIC ;
const uint64_t PLIC_BASE_ADDR = 0xc000000;
volatile PLIC* plic = (PLIC*)PLIC_BASE_ADDR;

volatile uint32_t trap_cnt = 0;

void external_irq_handler() {
    uint32_t claimed_id = plic->context_config[0].claimed_id;

    sample_timer->control = SAMPLE_TIMER_CONTROL_RESET;
    trap_cnt++;

    uint64_t mip = read_csr_mip();
    mip &= ~(1ull << 11);  // clear MEIP
    write_csr_mip(mip);
    plic->context_config[0].claimed_id = claimed_id;  // complete
}

void trap_handler(TrapContext* trap_ctx) {
    uint64_t mcause = read_csr_mcause();
    if (mcause == ((1ull << 63) | 11)) {  // machine external interrupt
        printf("interrupt happend...\n");
        external_irq_handler();
    }
    __traps_return(trap_ctx);
}

void plic_set_threshold(uint32_t context, uint32_t threshold) {
    plic->context_config[context].threshold = threshold;
}

void plic_set_priority(uint32_t interrupt_id, uint32_t priority) {
    plic->priority[interrupt_id] = priority;
}

void plic_enable_interrupt(uint32_t context, uint32_t interrupt_id) {
    plic->context_enable_bits[context][interrupt_id / 32] |= (1u << (interrupt_id % 32));
}

void plic_disenable_interrupt(uint32_t context, uint32_t interrupt_id) {
    plic->context_enable_bits[context][interrupt_id / 32] &= ~(1u << (interrupt_id % 32));
}

const uint32_t SAMPLE_TIMER_INTERRUPT_ID = 63;
int main() {
    TEST_START(__BASE_FILE__);
    printf("%x\n", sizeof(PLIC));
    trap_init();
    plic_set_priority(SAMPLE_TIMER_INTERRUPT_ID, 5);
    plic_set_threshold(0, 1); // 0 for machine mode interrupt
    plic_enable_interrupt(0, SAMPLE_TIMER_INTERRUPT_ID);

    sample_timer->interval_low = 1000;
    sample_timer->interval_high = 0;
    sample_timer->interrupt_mask = 0x1;  // enable interrupt
    while (trap_cnt < 10) {
    }

    pass();
    return 0;
}
