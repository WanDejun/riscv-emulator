#pragma once
// 定义 UART 内存映射地址 (QEMU virt 平台)
#include <stdarg.h>
#include <stdint.h>
#include "power.h"

#define UART_ADDR 0x10000000
#define UART_LSR_ADDR (UART_ADDR + 0x05)
#define UART_LSR_THRE 0x20  // Bit 5: Transmitter Holding Register Empty

// uart
void uart_putc(char ch);
uint8_t uart_getc();

// output
void print_str(const char* s);
void print_dec(long long val, int width, char pad_char);
void print_hex(unsigned long long val, int width, char pad_char);
void print_oct(unsigned long long val, int width, char pad_char);
void vprintf(const char* fmt, va_list ap);
void printf(const char* fmt, ...);

// input
long long input_dec();
uint8_t input_char();
int scanf(const char* fmt, ...);


#define TEST_START(x)                                                                    \
    print_str("========== START ");                                                      \
    print_str(x);                                                                        \
    print_str(" ==========\n");
#define TEST_END(x)                                                                      \
    print_str("========== END ");                                                        \
    print_str(x);                                                                        \
    print_str(" ==========\n");


#define PASS                                                                             \
    do {                                                                                 \
        print_str("\x1b[32mPASS\x1b[0m\n");                                              \
        PowerOff();                                                                      \
    } while (0)

#define FAIL                                                                             \
    do {                                                                                 \
        print_str("\x1b[30mFAIL\x1b[0m\n");                                              \
        PowerOff();                                                                      \
    } while (0)
