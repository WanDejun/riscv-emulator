// Author by: Claude Code Opus 4.8

#include "io.h"
#include "log.h"
#include "trap.h"
#include <stdint.h>

// Exercise the UART -> PLIC -> CPU interrupt path on the QEMU virt platform.
// The NS16550A asserts its IRQ while the transmit holding register is empty and
// the "THR empty" interrupt is enabled. THR is empty out of reset, so arming
// that interrupt should drive an external interrupt into machine mode at once.

// ---- NS16550A UART, byte-spaced registers (QEMU virt) ----
const uint64_t UART_BASE = 0x10000000;
const uint64_t UART_THR = UART_BASE + 0;  // write: transmit holding register
const uint64_t UART_IER = UART_BASE + 1;  // interrupt enable register
const uint64_t UART_IIR = UART_BASE + 2;  // read: interrupt identification

#define IER_RX_ENABLE 0x01  // received data available
#define IER_TX_ENABLE 0x02  // transmit holding register empty

#define IIR_PENDING 0x01  // 0 => an interrupt is pending
#define IIR_ID_MASK 0x0e  // interrupt id field
#define IIR_TX_EMPTY 0x02
#define IIR_RX_AVAIL 0x04

// UART0 interrupt source number on QEMU virt.
const uint32_t UART_IRQ = 10;

// ---- PLIC, machine-mode view of hart 0 == context 0 (QEMU virt) ----
const uint64_t PLIC_BASE = 0xc000000;
#define PLIC_PRIORITY(id) (PLIC_BASE + 4 * (id))
#define PLIC_ENABLE(ctx, id) (PLIC_BASE + 0x2000 + (ctx) * 0x80 + 4 * ((id) / 32))
#define PLIC_THRESHOLD(ctx) (PLIC_BASE + 0x200000 + (ctx) * 0x1000)
#define PLIC_CLAIM(ctx) (PLIC_BASE + 0x200004 + (ctx) * 0x1000)

const uint32_t M_CONTEXT = 0;

static inline void mmio_w8(uint64_t addr, uint8_t val) {
    *(volatile uint8_t*)addr = val;
}
static inline uint8_t mmio_r8(uint64_t addr) {
    return *(volatile uint8_t*)addr;
}
static inline void mmio_w32(uint64_t addr, uint32_t val) {
    *(volatile uint32_t*)addr = val;
}
static inline uint32_t mmio_r32(uint64_t addr) {
    return *(volatile uint32_t*)addr;
}

static volatile uint32_t uart_irq_count = 0;
static volatile uint8_t observed_iir = 0xff;

void trap_handler(TrapContext* trap_ctx) {
    uint64_t mcause = read_csr_mcause();

    // The only trap this test expects is the machine external interrupt.
    if (mcause != ((1ull << 63) | 11)) {
        printf("unexpected trap: mcause=%x mtval=%x\n", mcause, read_csr_mtval());
        fail();
    }

    // Claim the highest-priority pending interrupt from the PLIC.
    uint32_t irq = mmio_r32(PLIC_CLAIM(M_CONTEXT));
    if (irq == UART_IRQ) {
        // Record why the UART interrupted, then silence the source: the THRE
        // condition is level-triggered, so leaving it enabled would storm.
        observed_iir = mmio_r8(UART_IIR);
        mmio_w8(UART_IER, 0x00);
        uart_irq_count++;
    }
    // Signal completion so the PLIC can lower the interrupt.
    mmio_w32(PLIC_CLAIM(M_CONTEXT), irq);

    __traps_return(trap_ctx);
}

int main() {
    TEST_START(__BASE_FILE__);
    trap_init();

    // Route the UART interrupt to machine-mode hart 0 through the PLIC.
    mmio_w32(PLIC_PRIORITY(UART_IRQ), 1);    // non-zero priority enables the source
    mmio_w32(PLIC_THRESHOLD(M_CONTEXT), 0);  // accept any priority above 0
    mmio_w32(PLIC_ENABLE(M_CONTEXT, UART_IRQ),
             mmio_r32(PLIC_ENABLE(M_CONTEXT, UART_IRQ)) | (1u << (UART_IRQ % 32)));

    // Arm the "transmit holding register empty" interrupt; THR is empty at
    // reset, so the UART should raise its IRQ immediately.
    mmio_w8(UART_IER, IER_TX_ENABLE);

    // Wait for the handler to observe the interrupt. The watchdog turns a
    // missing interrupt into a FAIL instead of an indefinite hang.
    for (uint64_t watchdog = 0; uart_irq_count == 0; watchdog++) {
        if (watchdog > 100000000ull) {
            Log(ERROR, "UART interrupt never reached the CPU");
            fail();
        }
    }

    // The claimed source must be the UART and the IIR must report THR empty.
    if ((observed_iir & IIR_PENDING) != 0) {
        Log(ERROR, "IIR reports no pending interrupt: %x", observed_iir);
        fail();
    }
    if ((observed_iir & IIR_ID_MASK) != IIR_TX_EMPTY) {
        Log(ERROR, "unexpected interrupt id, IIR=%x", observed_iir);
        fail();
    }

    Log(INFO, "UART interrupt handled (IIR=%x, count=%d)", observed_iir, uart_irq_count);
    pass();
    return 0;
}
