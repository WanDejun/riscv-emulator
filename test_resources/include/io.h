// 定义 UART 内存映射地址 (QEMU virt 平台)
#include <stdarg.h>
#define UART_ADDR 0x10000000
#define UART_LSR_ADDR (UART_ADDR + 0x05)
#define UART_LSR_THRE 0x20 // Bit 5: Transmitter Holding Register Empty

void uart_putc(char ch);

void print_dec(long long val, int width, char pad_char);
void print_hex(unsigned long long val, int width, char pad_char);
void print_oct(unsigned long long val, int width, char pad_char);
void vprintf(const char *fmt, va_list ap);
void printf(const char *fmt, ...);

// 输出字符串
void print_str(const char *s);