#include "io.h"
#include <stdarg.h>
#include <stdint.h>

// 定义 UART 内存映射地址 (QEMU virt 平台)
#define UART_ADDR 0x10000000
#define UART_LSR_ADDR (UART_ADDR + 0x05)
#define UART_LSR_THRE 0x20  // Bit 5: Transmitter Holding Register Empty
#define UART_LSR_RDR 0x01   // Bit 5: Receive Data Ready

#define INPUT_LINEBUF_SIZE 128

void uart_putc(char ch) {
    volatile char* uart_tx = (volatile char*)UART_ADDR;
    volatile uint8_t* uart_lsr = (volatile uint8_t*)UART_LSR_ADDR;

    while ((*uart_lsr & UART_LSR_THRE) == 0)
        ;  // 等待发送缓冲区为空

    *uart_tx = ch;

    // 如果是 '\n'，自动补一个 '\r' 让终端正确换行
    if (ch == '\n') {
        while ((*uart_lsr & UART_LSR_THRE) == 0)
            ;
        *uart_tx = '\r';
    }
}

// with line buffer, echo, '\r' -> '\r\n'
uint8_t uart_getc() {
    static char linebuf[INPUT_LINEBUF_SIZE];
    static int buf_len = 0;
    static int buf_pos = 0;

    // 如果上次缓冲区里还有没读完的字符，直接返回
    if (buf_pos < buf_len) {
        return (uint8_t)linebuf[buf_pos++];
    }

    // 否则重新读一行
    buf_len = 0;
    buf_pos = 0;

    while (1) {
        volatile char* uart_rx = (volatile char*)UART_ADDR;
        volatile uint8_t* uart_lsr = (volatile uint8_t*)UART_LSR_ADDR;

        // 等待接收
        while ((*uart_lsr & UART_LSR_RDR) == 0)
            ;

        uint8_t data = *uart_rx;

        // 回显
        uart_putc(data);
        if (data == '\r') {
            uart_putc('\n');
            data = '\n';  // 转换成 '\n' 作为统一行结束
        }

        // 处理退格（可选功能）
        if (data == '\b' || data == 127) {
            if (buf_len > 0) {
                buf_len--;
                // 回显退格修正
                uart_putc('\b');
                uart_putc(' ');
                uart_putc('\b');
            }
            continue;
        }

        linebuf[buf_len++] = data;

        // 行结束 → 完成一行
        if (data == '\n' || data == '\r' || buf_len >= INPUT_LINEBUF_SIZE - 1) {
            break;
        }
    }

    // 返回这一行的第一个字符
    return (uint8_t)linebuf[buf_pos++];
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
        buf[i++] = '0' + (val & 7);  // 取最低3位
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

void vprintf(const char* fmt, va_list ap) {
    while (*fmt) {
        if (*fmt != '%') {
            uart_putc(*fmt++);
            continue;
        }

        fmt++;  // skip '%'

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
        int long_flag = 0;  // 0=default, 1=long, 2=long long
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
            const char* s = va_arg(ap, const char*);
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
            uart_putc(*(fmt - 1));  // 打印未知格式符
        }
    }
}

void printf(const char* fmt, ...) {
    va_list ap;
    va_start(ap, fmt);

    vprintf(fmt, ap);

    va_end(ap);
}

// 输出字符串
void print_str(const char* s) {
    while (*s) {
        uart_putc(*s++);
    }
}

#define isdigit(__n) ((__n) >= '0' && (__n) <= '9')
#define isalpha(__n) (((__n) >= 'A' && (__n) <= 'Z') || ((__n) >= 'a' && (__n) <= 'z'))
#define isspace(__n)                                                                     \
    ((__n) == ' ' || (__n) == '\t' || (__n) == '\n' || (__n) == '\v' || (__n) == '\f' || \
     (__n) == '\r')
#define islower(__n) ((__n) >= 'a' && (__n) <= 'z')
#define isupper(__n) ((__n) >= 'A' && (__n) <= 'Z')
#define isprint(__n) ((__n) >= 0x20 && (__n) <= 0x7E)

static uint8_t glimpse = -1;

long long input_dec() {
    long long num = 0;
    while (!isdigit(glimpse)) {
        glimpse = uart_getc();
    }
    while (isdigit(glimpse)) {
        num = num * 10;
        num += glimpse - '0';
        glimpse = uart_getc();
    }

    return num;
}

uint8_t input_char() {
    while (!isprint(glimpse)) {
        glimpse = uart_getc();
    }
    uint8_t c = glimpse;
    glimpse = uart_getc();
    return c;
}

int scanf(const char* fmt, ...) {
    va_list args;
    va_start(args, fmt);

    int count = 0;

    for (const char* p = fmt; *p; p++) {
        if (*p != '%')
            continue;  // 非格式化字符忽略

        p++;  // 看下一个字符
        if (*p == 'd') {
            int* ip = va_arg(args, int*);
            *ip = (int)input_dec();
            count++;
        }
        else if (*p == 'l') {
            p++;
            if (*p == 'd') {
                long* lp = va_arg(args, long*);
                *lp = (long)input_dec();
                count++;
            }
            else if (*p == 'l' && *(p + 1) == 'd') {
                long long* llp = va_arg(args, long long*);
                *llp = input_dec();
                count++;
                p++;  // 多走一步（匹配到 "lld"）
            }
        }
        else if (*p == 'c') {
            char* cp = va_arg(args, char*);
            *cp = input_char();
            count++;
        }
    }

    va_end(args);
    return count;
}
