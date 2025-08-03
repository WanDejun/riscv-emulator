#include "io.h"
#include <stdarg.h>
#include <stdint.h>

// 定义 UART 内存映射地址 (QEMU virt 平台)
#define UART_ADDR 0x10000000
#define UART_LSR_ADDR (UART_ADDR + 0x05)
#define UART_LSR_THRE 0x20 // Bit 5: Transmitter Holding Register Empty

void uart_putc(char ch) {
  volatile char *uart_tx = (volatile char *)UART_ADDR;
  volatile uint8_t *uart_lsr = (volatile uint8_t *)UART_LSR_ADDR;

  while ((*uart_lsr & UART_LSR_THRE) == 0)
    ; // 等待发送缓冲区为空

  *uart_tx = ch;

  // 如果是 '\n'，自动补一个 '\r' 让终端正确换行
  if (ch == '\n') {
    while ((*uart_lsr & UART_LSR_THRE) == 0)
      ;
    *uart_tx = '\r';
  }
}

void print_dec(long long val, int width, char pad_char) {
  char buf[32];
  int i = 0;
  int neg = 0;

  if (val < 0) {
    neg = 1;
    val = -val;
  }

  do {
    buf[i++] = '0' + val % 10;
    val /= 10;
  } while (val);

  if (neg) {
    buf[i++] = '-';
  }

  while (i < width) {
    buf[i++] = pad_char;
  }

  // 反向输出
  while (i--)
    uart_putc(buf[i]);
}

void print_hex(unsigned long long val, int width, char pad_char) {
  char buf[32];
  int i = 0;
  char hex[] = "0123456789abcdef";

  do {
    buf[i++] = hex[val % 16];
    val /= 16;
  } while (val);

  if (pad_char == '0') {
    uart_putc('0');
    uart_putc('x');
  }

  while (i < width) {
    uart_putc(pad_char);
    width--;
  }

  if (pad_char != '0') {
    uart_putc('0');
    uart_putc('x');
  }

  while (i--) {
    uart_putc(buf[i]);
  }
}

void print_oct(unsigned long long val, int width, char pad_char) {
  char buf[32];
  int i = 0;

  do {
    buf[i++] = '0' + (val & 7); // 取最低3位
    val >>= 3;
  } while (val);

  if (pad_char == '0')
    uart_putc('0');

  while (i < width) {
    uart_putc(pad_char);
    width--;
  }

  if (pad_char == ' ')
    // optional: 前缀显示？
    uart_putc('0');

  while (i--) {
    uart_putc(buf[i]);
  }
}

void vprintf(const char *fmt, va_list ap) {
  while (*fmt) {
    if (*fmt != '%') {
      uart_putc(*fmt++);
      continue;
    }

    fmt++; // skip '%'

    // 解析填充字符
    char pad_char = ' ';
    if (*fmt == '0') {
      pad_char = '0';
      fmt++;
    }

    // 解析宽度
    int width = 0;
    while (*fmt >= '0' && *fmt <= '9') {
      width = width * 10 + (*fmt - '0');
      fmt++;
    }

    // 解析长度修饰符
    int long_flag = 0; // 0=default, 1=long, 2=long long
    if (*fmt == 'l') {
      fmt++;
      long_flag = 1;
      if (*fmt == 'l') {
        fmt++;
        long_flag = 2;
      }
    }

    // 解析格式符号
    switch (*fmt++) {
    case 'd': {
      long long val;
      if (long_flag == 2)
        val = va_arg(ap, long long);
      else if (long_flag == 1)
        val = va_arg(ap, long);
      else
        val = va_arg(ap, int);
      print_dec(val, width, pad_char);
      break;
    }

    case 'o': {
      unsigned long long val;
      if (long_flag == 2)
        val = va_arg(ap, unsigned long long);
      else if (long_flag == 1)
        val = va_arg(ap, unsigned long);
      else
        val = va_arg(ap, unsigned int);
      print_oct(val, width, pad_char);
      break;
    }

    case 'x': {
      unsigned long long val;
      if (long_flag == 2)
        val = va_arg(ap, unsigned long long);
      else if (long_flag == 1)
        val = va_arg(ap, unsigned long);
      else
        val = va_arg(ap, unsigned int);
      print_hex(val, width, pad_char);
      break;
    }

    case 's': {
      const char *s = va_arg(ap, const char *);
      while (*s)
        uart_putc(*s++);
      break;
    }

    case 'c': {
      const char c = va_arg(ap, int);
      uart_putc(c);
      break;
    }

    case '%': {
      uart_putc('%');
      break;
    }

    default:
      uart_putc('%');
      uart_putc(*(fmt - 1)); // 打印未知格式符
    }
  }
}

void printf(const char *fmt, ...) {
  va_list ap;
  va_start(ap, fmt);

  vprintf(fmt, ap);

  va_end(ap);
}

// 输出字符串
void print_str(const char *s) {
  while (*s) {
    uart_putc(*s++);
  }
}