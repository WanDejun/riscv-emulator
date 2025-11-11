#include "trap.h"
#include "io.h"
#include <stdint.h>

typedef struct TEST_DEVICE_T {
    uint32_t icr;
    uint32_t imr;
    uint32_t idr0;
    uint32_t idr1;
} TestDevice;
const uint64_t TEST_DEVICE_BASE_ADDR = 0x101000;
volatile TestDevice* test_device = (TestDevice*)TEST_DEVICE_BASE_ADDR;

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
        
    trap_cnt++;

    uint64_t mip = read_csr_mip();
    mip &= ~(1 << 11);  // clear MEIP
    write_csr_mip(mip);
    plic->context_config[0].claimed_id = claimed_id;  // complete
}

void trap_handler(TrapContext* trap_ctx) {
    uint64_t mcause = read_csr_mcause();
    if (mcause == (1ll << 63 | 11)) {  // machine external interrupt
        printf("interrupt happend...\n");
        external_irq_handler();
    }
    __traps_return(trap_ctx);
}

const uint64_t PLIC_CONTEXT_CONFIG_OFFSET = 0x200000;
const uint64_t PLIC_CONTEXT_CONFIG_SIZE = 0x1000;
void plic_set_threshold(uint32_t context, uint32_t threshold) {
    uint64_t addr = PLIC_CONTEXT_CONFIG_OFFSET + (context * PLIC_CONTEXT_CONFIG_SIZE);
    plic->context_config[context].threshold = threshold;
}

void plic_set_priority(uint32_t interrupt_id, uint32_t priority) {
    plic->priority[interrupt_id] = priority;
}

void plic_enable_interrupt(uint32_t context, uint32_t interrupt_id) {
    uint64_t addr = 0x2000 + (context * 0x1000) + (interrupt_id / 32) * 4;
    plic->context_enable_bits[context][interrupt_id / 32] |= (1 << (interrupt_id % 32));
}

void plic_disenable_interrupt(uint32_t context, uint32_t interrupt_id) {
    uint64_t addr = 0x2000 + (context * 0x1000) + (interrupt_id / 32) * 4;
    plic->context_enable_bits[context][interrupt_id / 32] &= ~(1 << (interrupt_id % 32));
}

const uint32_t TEST_DEVICE_INTERRUPT_ID = 63;
int main() {
    printf("%x\n", sizeof(PLIC));
    trap_init();
    plic_set_priority(TEST_DEVICE_INTERRUPT_ID, 5);
    plic_set_threshold(0, 1);
    plic_enable_interrupt(0, TEST_DEVICE_INTERRUPT_ID);

    test_device->idr0 = 0x100,000,000;
    test_device->imr = 0x1;  // enable interrupt

    while (trap_cnt < 10) {
        
    }

    PowerOff();
    return 0;
}