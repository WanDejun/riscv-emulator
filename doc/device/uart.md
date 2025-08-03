# UART 16550

## 寄存器概述

### LCR(Line Control Register   +0x0C):

控制串口的行为

| bit   | name                      |   Description                                     |
| :-:   | :-:                       |   :-:                                             |
| 0-1   | Word Length               | 00: 5bits; 01: 6bits; 10: 7bits; 11: 8bits;       |
| 2     | Stop Bit                  | 0: 1bits; 1: 1.5 (5 Word Length) / 2 (others) bits|
| 3     | Parity Enable             | 0 : disable; 1: enable;                           |
| 4     | Even Parity               | 0: Odd Parity; 1: Even Parity;                    |
| 5     | Stick Parity              | NULL                                              |
| 6     | Break Control             | 1: Set TXD to idle statu                          |
| 7     | divisor latch Access Bit  | Allow to Access DLL/DLM/FCR(RO) reg               |

### `IER`(Interrupt Enable Register +0x04):

控制中断的行为

| bit   |   Description                 |
| :-:   | :-:                           |
| 0     | Receive Data Available        |
| 1     | Transmitter Buffer Empty      |
| 2     | Line Status                   |
| 3     | Modem Status                  |

`IER` = `Line Status` 时, 由 `LSR` 控制中断行为
`IER` = `Modem Status` 时, 由 `MSR` 控制中断行为

### `LSR`(Line Status Register +0x14):

| Register  |  Bit7	        | Bit6	        | Bit5                  | Bit4              | Bit3          | Bit2          | Bit1          | Bit0              |
| :-:       |  :-:	        | :-:	        | :-:                   | :-:               | :-:           | :-:           | :-:           | :-:               |
| LSR	    | 0/FIFO Error	| transmit empty| transmit holding empty| break interrupt   | framing error | parity error  | overrun error | receive data ready|

### DLM & DLL:
divisior = DLM << 32 | DLL
divisior = input_frequency / (16 × target_frequency)

即每 (input_frequency) / (16 * divisior) 采集一个输入, 切换一个输出. 每 input_frequency / divisior 进行一次输入采集, 16个值取均值.

## FIFO

### TODO

- [ ] TODO