.section .text.ENTRY
.globl _start

_start:
    la sp, stack_top        # 设置栈指针
    #claer bss
    la a0, stack_lower_bound
    la a1, stack_top
1:
    beq a0, a1, 2f
    sd zero, 0(a0)
    addi a0, a0, 8
    j 1b
2:

    call main               # 调用主函数

    # 退出
    li a7, 93               # exit syscall 编号
    li a0, 0                # 退出码 0
    ecall

    .align 3
    .section .stack
    .globl stack_lower_bound
stack_lower_bound:
    .space 4096 * 16       # 512KB 栈空间 (不够就接着开)
    .globl stack_top
stack_top: