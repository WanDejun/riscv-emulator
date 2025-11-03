#include "io.h"
#include "syscall.h"
#include "trap.h"
#include <stdint.h>

int ecall_args[10][8];
int ecall_id[10];
int standard[10] = { 5, 7, 4, 6 };
int ecall_cnt;

void trap_handler(TrapContext* trap_ctx) {
    int mcause = read_csr_mcause();
    if (mcause == 11) {  // m-ecall
        ecall_id[ecall_cnt] = trap_ctx->x[17];
        for (int i = 0; i < 7; i++) {
            ecall_args[ecall_cnt][i] = trap_ctx->x[10 + i];
        }
        ecall_cnt++;
        trap_ctx->mepc += 4;
    }
    __traps_return(trap_ctx);
}

int main() {
    TEST_START(__BASE_FILE__);
    trap_init();

    __syscall(10);
    __syscall(11, 1);
    __syscall(12, 1, 2);
    __syscall(13, 1, 2, 3);
    __syscall(14, 1, 2, 3, 4);
    __syscall(15, 1, 2, 3, 4, 5);
    __syscall(16, 1, 2, 3, 4, 5, 6);

    for (int i = 0; i < 7; i++) {
        printf("[%d]: ecall_nr: %d\n\t", i, ecall_id[i]);
        if (ecall_id[i] != 10 + i) {
            FAIL;
        }
        printf("args: ");
        for (int j = 0; j < i; j++) {
            printf("%2d ", ecall_args[i][j]);
            if (ecall_args[i][j] != j + 1) {
                FAIL;
            }
        }
        printf("\n");
    }

    TEST_END(__BASE_FILE__);
    PASS;

    return 0;
}