# RISCV-EMULATOR

## About `RISCV-EMULATOR`

`riscv-emulator` is a simple full-system emulator developed by rust :fire:.  

The main feature of `riscv-emulator` include:

- Support ISA:
  - riscv64: rv64-imf, Zicsr
- A tiny debugger monitor:
  - single step
  - register / memory examination
  - break point
  - better disassembly
- Memory
- Memory Mapped I/O
- 3 devices:
  - serial, virtio-blk, timer
- MMU
- CLINT and interrupt
- free-standing elf is supported.

## Build

### Install building toolchain

We use rust-nightly as development toolchain for emulator. And use [riscv-gnu-toolchain](https://github.com/riscv-collab/riscv-gnu-toolchain) to build some test.  

For ArchLinux: 

```sh
sudo pacman -S rustup
rustup default nightly
```

### Run Emulator

- `-g` to enbale tiny debugger (default = false)
- `-loglevel`
  - error
  - warn
  - info (default)
  - debug
  - trace
- `--device`
  - --device:virtio-device:/dev/null
- input executalbe file.

```sh
cargo run ./test_resources/bin/virtio_blk_test.elf --device=virtio-block:./tmp/img_blk -g --loglevel=debug
```

## Virt Board

### MMIO Address

| device | address base | address length |
| :-: | :-: | :-: |
| `power-manager`   | 0x0010_0000   | 0x02      |
| `uart`            | 0x1000_0000   | 0x08      |
| `clint`           | 0x0200_0000   | 0x10000   |
| `virtio`          | 0x1000_1000   | 0x1000    |
| `ram`             | 0x8000_0000   | 0x800_0000|