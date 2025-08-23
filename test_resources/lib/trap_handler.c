#include "io.h"
#include "power.h"
#include "trap.h"
#include <stdint.h>

typedef struct TRAP_CONTEXT {
    uint64_t x[32];
    uint64_t mstatus;
    uint64_t mepc;
    uint64_t mscratch;
} TrapContext;

void __traps_entry();
void __traps_return(TrapContext*);

#define TRAP_STACK_SIZE 0x4000
static uint8_t TRAP_STACK[TRAP_STACK_SIZE];
TrapContext* TRAP_CONTEXT = (TrapContext*)(TRAP_STACK + sizeof(TrapContext));

// 使用 X 宏生成读取函数
#define X(name, addr) \
    static inline uint64_t read_csr_##name(void) \
    { \
        uintptr_t result; \
        asm volatile ( \
            "csrr %0, %1" \
            : "=r" (result) \
            : "i" (addr) \
        ); \
        return result; \
    }
CSR_LIST
#undef X

// 使用 X 宏生成写入函数
#define X(name, addr) \
    static inline void write_csr_##name(uint64_t value) \
    { \
        asm volatile ( \
            "csrw %0, %1" \
            : \
            : "i" (addr), "r" (value) \
        ); \
    }
CSR_LIST
#undef X

// just display trap val and then PowerOff.
__attribute__((weak))
void trap_handler(TrapContext* trap_ctx) {
    printf("mcause: %x\n", read_csr_mcause());
    printf("mtval: %x\n", read_csr_mtval());
    trap_ctx->mepc = (uint64_t)(PowerOff);
    __traps_return(trap_ctx);
}

void trap_init() {
    write_csr_mtvec((uint64_t)(*__traps_entry));
    write_csr_mscratch((uint64_t)(TRAP_STACK + TRAP_STACK_SIZE));
}
