#include <stdint.h>

// RISC-V CSR 寄存器编号定义（根据特权架构规范）
#define CSR_MSTATUS   0x300
#define CSR_MIE       0x304
#define CSR_MTVEC     0x305
#define CSR_MSCRATCH   0x340
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

// X 宏定义所有 CSR
#define CSR_LIST \
    X(mstatus, CSR_MSTATUS) \
    X(mie, CSR_MIE) \
    X(mtvec, CSR_MTVEC) \
    X(mscratch, CSR_MSCRATCH) \
    X(mepc, CSR_MEPC) \
    X(mcause, CSR_MCAUSE) \
    X(mtval, CSR_MTVAL) \
    X(mip, CSR_MIP) \

// 使用 X 宏生成读取函数
#define X(name, addr) \
    static inline uintptr_t read_csr_##name(void);
CSR_LIST
#undef X

// 使用 X 宏生成写入函数
#define X(name, addr) \
    static inline void write_csr_##name(uintptr_t value);
CSR_LIST
#undef X

void trap_init();
