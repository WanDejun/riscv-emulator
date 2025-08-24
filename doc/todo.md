# TODO List

## Urgent

- [ ] 添加 `readme`.

## High Priority

- [ ] 在 `PowerOff` 之后退出 `Debugger`.  
- [ ] Fix: `fast_uart::test` 的多线程的全局 `SIMULATION_IO` 竞争问题. (将 `uart` 的 `Sender` 和 `Receiver` 接口暴露出来).

## Medium Priority

- [ ] 将 `SignedExtend` 提前到 `ID` 阶段, 通过 i-cache 加速.
- [ ] Fix: 修复 `riscv32` 支持.

## Low Priority

- [ ] `Execute` 内联汇编加速.
- [ ] 添加 `gdb server`.
- [ ] 添加对 `bin` 文件的支持.
