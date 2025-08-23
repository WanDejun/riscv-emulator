#include "io.h"
#include <stdint.h>
typedef struct TRAP_CONTEXT {
    uint64_t x[32];
    uint64_t sstatus;
    uint64_t sepc;
    uint64_t sscratch;
} TrapContext;

void __traps_entry();
void __traps_return(TrapContext*);

#define TRAP_STACK_SIZE 0x4000
static uint8_t TRAP_STACK[TRAP_STACK_SIZE];
TrapContext* TRAP_CONTEXT = (TrapContext*)(TRAP_STACK + sizeof(TrapContext));

#define VIRT_POWEROFF_ADDR 0x100000
void PowerOff() {
    uart_putc('\n');
    volatile uint32_t* poweroff = (uint32_t*)VIRT_POWEROFF_ADDR;
    *poweroff = 0x5555;
    while (1) { /* 等待 QEMU 退出 */
    }
}

/**​
 * 使用内联汇编读取 CSR 寄存器
 * @param csr_num: CSR 寄存器编号
 * @return: CSR 寄存器的当前值
 */
static inline uintptr_t read_csr(const uintptr_t csr_num)
{
    uintptr_t result;
    
    // 使用 csrr 指令读取 CSR
    asm volatile (
        "csrr %0, %1"        // 汇编指令：将 CSR 的值读取到 result
        : "=r" (result)      // 输出操作数：result 变量使用寄存器约束
        : "i" (csr_num)      // 输入操作数：csr_num 作为立即数
    );
    
    return result;
}

/**
 * 使用内联汇编写入 CSR 寄存器
 * @param csr_num: CSR 寄存器编号
 * @param value: 要写入的值
 */
static inline void write_csr(const uintptr_t csr_num, const uintptr_t value)
{
    // 使用 csrw 指令写入 CSR
    asm volatile (
        "csrw %0, %1"        // 汇编指令：将 value 写入 CSR
        :                    // 无输出操作数
        : "i" (csr_num), "r" (value)  // 输入操作数
    );
}

// RISC-V CSR 寄存器编号定义（根据特权架构规范）
#define CSR_MSTATUS   0x300
#define CSR_MIE       0x304
#define CSR_MTVEC     0x305
#define CSR_MCRATCH   0x340
#define CSR_MEPC      0x341
#define CSR_MCAUSE    0x342
#define CSR_MTVAL     0x343
#define CSR_MIP       0x344
#define CSR_SSTATUS   0x100
#define CSR_SIE       0x104
#define CSR_STVEC     0x105
#define CSR_SEPC      0x141
#define CSR_SCAUSE    0x142
#define CSR_STVAL     0x143
#define CSR_SIP       0x144
#define CSR_SATP      0x180

void trap_handler(TrapContext* trap_ctx) {
    printf("%ld", read_csr(CSR_MCAUSE));
    write_csr(CSR_MEPC, (uint64_t)(*PowerOff));
    __traps_return(trap_ctx);
}

void trap_init() {
    write_csr(CSR_STVEC, (uint64_t)(*__traps_entry));
    write_csr(CSR_MCRATCH, (uint64_t)(TRAP_STACK + TRAP_STACK_SIZE));
}

int main() {
    uint64_t* illigal_ptr = (uint64_t*)(0x11110000);
    uint64_t val = *illigal_ptr;   
}