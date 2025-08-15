# RISC-V Control and Status Registers (CSRs)

## csr 简介

- *具体内容自行查看 `The RISC-V Instruction Set Manual: Volume II` `Ch2, Ch3, Ch12`.*

`csr` 用于控制 `riscv` 处理器的行为和提供处理器内部所需的信息寄存, 如: 中断向量表位置, 中断异常标志, 中断辅助值, 浮点处理单元行为控制等.

## `csr` 访问以及 `csr` 映射表.

### 访问指令

- *具体内容自行查看 `The RISC-V Instruction Set Manual: Volume I` `Ch7`.*

所有 `csr instructions` 均为 I 指令, 立即数部分为 `csr` 寄存器地址(0xfff-0x000). `csr` 与普通的寄存器不同, 在正常执行用户指令的情况下不应该被使用, 因此需要设置一组特定的指令用于访问 csr:

|name|description| `rd` is `x0` | `rs` is `x0`/ `uimm` == 0 |
|:-:|:-:|:-:|:-:|
| **所有 `csr` 指令都能读取 `csr` 并进行0扩展至 `rd`**, 但是在 `rd` == `x0` 时行为不同. 当 `mask` 为 `x0` |
| `CSRR?I` 指令是 `CSRR?` 的变体, `I` 变体将 `rs` 字段视为 `uimm[4:0]`, 等效于 `X[rs]` |
| `CSRRW` | **write**: 将 `rs` 值写入 `csr`. | 不会读取 `csr`, 不会产生读取的副作用 | 写入0 |
| `CSRRS` | **set**: 将 `rs` 作为 `mask`, 将值设为 1. | 不会读取 `csr`, 不会产生读取的副作用 | 不会写入 `csr`, 不会产生副作用 |
| `CSRRC` | **clear**: 将 `rs` 作为 `mask`, 将值设为 0. | 不会读取 `csr`, 不会产生读取的副作用 | 不会写入 `csr`, 不会产生副作用 |
| `CSRRWI` | **write**: 将 `uimm` 值写入 `csr`. | 不会读取 `csr`, 不会产生读取的副作用 | 写入0 |
| `CSRRSI` | **set**: 将 `uimm` 值作为 `mask`, 将值设为 1. | **会读取 `csr`, 并产生副作用, 但是不会写入 `x0`** | 不会写入 `csr`, 不会产生副作用 |
| `CSRRCI` | **clear**:  将 `uimm` 值作为 `mask`, 将值设为 0. | **会读取 `csr`, 并产生副作用, 但是不会写入 `x0`** | 不会写入 `csr`, 不会产生副作用 |

所有读行为发生在修改值之前. 若要只读取一个值到寄存器, 使用 `CSRRS` or `CSRRC`, 若只想清除 `csr` 的读标志, 使用 `CSRRSI` | `CSRRCI`, 将 `rd` 设为 `x0`.


### csr 地址

- *具体内容自行查看 `The RISC-V Instruction Set Manual: Volume II` `Ch2`,

`csr` 指令通过 `imm` 字段的12位立即数访问特定 `csr` 寄存器, 因此最大支持的 `csr` 数量为 4096 (实际使用的远小于4096).

`csr` 更具访问权限不同, 和 cpu 一样分为 `M/S/U/V` 四个模式(~~用户模式不能更改内核设置 <- 废话~~). 其中每个模式都有固定的地址段:

|CSR Address|Use and Accessibility|
|:-:|:-:|
|0x000-0x0ff| Unprivileged and User-Level CSRs |
|0x100-0x1ff| Supervisor-Level CSRs |
|0x200-0x2ff| Hypervisor and VS CSRs |
|0x300-0x3ff| Machine-Level CSRs |
|0x400-0xfff| 比较杂乱, 自行查看手册, 大多归属某个扩展指令或自定义 |

#### `M-Mode` 的 `csr`

|name| address | description |
|:-:|:-:|:-:|
| Machine Trap Setup |
| mstatus   | 0x300 | Machine status register. |
| misa      | 0x301 | ISA and extensions  |
| medeleg   | 0x302 | Machine exception delegation register. (Not Used if M-Mode only) |
| mideleg   | 0x303 | Machine interrupt delegation register. (Not Used if M-Mode only) |
| mie       | 0x304 | Machine interrupt-enable register.  |
| mtvec     | 0x305 | Machine trap-handler base address.  |
| mcounteren| 0x306 | Machine counter enable.  |
| mstatush  | 0x310 | Additional machine status register, RV32 only.  |
| medelegh  | 0x312 | Upper 32 bits of medeleg, RV32 only.  |
| Machine Trap Handling |
| mscratch  | 0x340 | Machine scratch register. |
| mepc      | 0x341 | Machine exception program counter. |
| mcause    | 0x342 | Machine trap cause. |
| mtval     | 0x343 | Machine trap value. |
| mip       | 0x344 | Machine interrupt pending. |
| mtinst    | 0x34A | Machine trap instruction (transformed) |
| mtval2    | 0x34B | Machine second trap value. |
| Machine Configuration |
| 查阅手册 |
| Machine Memory Protection |
| 查阅手册 |
| Machine State Enable Registers |
| 查阅手册 |

## 陷入流程 (中断/异常)

### 触发条件

riscv 将中断与异常统一处理, 但是在 cpu 产生陷入的流程上略有不同.

#### 中断

riscv 的 `mie csr` 为一个 `mask`, 用与控制某一路中断的开关.
riscv 的 `mip csr` 为中断信号, 当 `mip` 的第 `i` 位为 1, 则表示第 i 条中断信号线在等待中断处理.

当 (mie & mip) != 0 时, 会产生中断, 中断号越高, 优先级越高. 
其中 `mip/mie` 的低12位为固定标准中断:

| 编码 (code)    |  符号 |描述                            |优先级|
|:-:            |:-:    |:-:                            |:-:|
| 0	            |USI    | User Software Interrupt       |低|
| 1	            |SSI    | Supervisor Software Interrupt |↑|
| 3	            |MSI    | Machine Software Interrupt    |↑|
| 4	            |UTI    | User Timer Interrupt          |↑|
| 5	            |STI    | Supervisor Timer Interrupt    |↑|
| 7	            |MTI    | Machine Timer Interrupt       |↑|
| 8	            |UEI    | User External Interrupt       |↑|
| 9	            |SEI    | Supervisor External Interrupt |↑|
| 11            |MEI    | Machine External Interrupt    |高|

#### 异常

异常的优先级低于中断.

### 流程

1. 在指令执行过程中遇到异常, cpu 向 mcause 写入异常号
2. 一条指令执行完之后, cpu 检测到中断, 向 mcause 写入中断号
3. 写入 mepc, 保存当前/下一条的 pc (指令异常可能需要重新执行).
4. 写入 mtval, 附加陷入信息.
5. mstatus 更新: 保存 MIE(全局中断使能位)到 MPIE -> 关闭 MIE(防止嵌套中断，除非手动重新开)
6. 更具 `mtvec` 决定跳转地址, Direct 模式 / Vectored 模式.
7. 软件保存现场, 其中 mscratch 用于辅助保存现场信息(sp 或是 一个跳板地址, 将当前现场保存在 sp 或是 跳板地址上)
7. 执行 trap_handler.
8. 软件恢复现场.
9. 通过 mret 返回: mstatus.MIE ← mstatus.MPIE, mstatus.MPIE ← 1, pc ← mepc
