# RISC-V Emulator

## About

`RISC-V Emulator` is a educational full-system emulator for the RISC-V architecture, developed in Rust :fire:.

The main features of `RISC-V Emulator` include:

- Supported ISA:
  - riscv64: rv64-imf + Zicsr
- A tiny debugger monitor:
  - Single step execution
  - Register / memory examination
  - Breakpoint
  - Basic disassembly
- Memory / Memory Mapped I/O
- MMU
- Devices:
  - Serial, VirtIO-blk, Timer
- CLINT and interrupt handling
- Free-standing ELF

## Build

Install Rust nightly, for example, in Arch Linux:

```sh
sudo pacman -S rustup
rustup default nightly
```

Build:

```sh
cargo build
```

## Testing

We use [riscv-tests](https://github.com/riscv-software-src/riscv-tests) as our test suite. To compile the tests, you'll need [riscv-gnu-toolchain](https://github.com/riscv-collab/riscv-gnu-toolchain) and follow the instructions in riscv-tests' README.

Then, run `cargo test -F riscv-tests`.

## Usage

### Quick Start

```sh
# Build demo, make sure you have compiler for RISC-V
cd ./test_resources && make

# Run a simple program
cargo run -- ./bin/main.elf

# Run with debugger enabled
cargo run -- ./bin/main.elf -g
```

### Command Line Options

- `-h, --help`: Show help messages
- `-g`: Enable tiny debugger (default: false)
- `--loglevel <LEVEL>`: Set log level
- `--device <TYPE:PATH>`: Configure device
  - Example: `--device=virtio-block:/path/to/image`
- `<EXECUTABLE>`: Path to the ELF executable file

### Example Usage

```sh
cargo run -- ./test_resources/bin/virtio_blk_test.elf --device=virtio-block:./tmp/img_blk -g --loglevel=debug
```

## Virt Board

### MMIO Address Map

| Device | Address Base | Address Length |
| :-: | :-: | :-: |
| `power-manager`   | 0x0010_0000   | 0x02      |
| `uart`            | 0x1000_0000   | 0x08      |
| `clint`           | 0x0200_0000   | 0x10000   |
| `virtio`          | 0x1000_1000   | 0x1000    |
| `ram`             | 0x8000_0000   | 0x800_0000|

## License

This project is licensed under the MIT License.

---

The "RISC-V" trade name is a registered trademark of RISC-V International. This project is not affiliated with, endorsed by, or sponsored by RISC-V International. For more information about RISC-V, please see [https://riscv.org](https://riscv.org).