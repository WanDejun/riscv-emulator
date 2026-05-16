# RISC-V Emulator

## About

`RISC-V Emulator` is an educational full-system emulator for the RISC-V architecture, written in Rust :fire:.

The main features of `RISC-V Emulator` include:

- Supported ISA:
  - RV64G (RV64IMAFD, Zicsr, Zifencei)
- Supported privilege modes:
  - M, S, and U modes
- A simple debugger monitor called rvdb
- GDB support
- Virtual memory
- Devices:
  - CLINT, PLIC, serial, and VirtIO-blk (VirtIO atomicity is currently broken)

An online version with the emulator's core functionality is also available: [rvemu-web](https://blog.satori-march.top/rvemu-web).

## Build

Install Rust nightly, for example, on Arch Linux:

```sh
sudo pacman -S rustup
rustup default nightly
```

Build:

```sh
cargo build
```

## Testing

We use [riscv-tests](https://github.com/riscv-software-src/riscv-tests) as our test suite. To build the tests, install [riscv-gnu-toolchain](https://github.com/riscv-collab/riscv-gnu-toolchain) and follow the instructions in the riscv-tests README.

Then, run `cargo test --features riscv-tests`.

Test support for `riscv-arch-test` also exists, but it is not integrated into CI. Unfortunately, the test suite stabilized at 4.x a few months after we implemented support for 3.x, so the suite we use is not up to date at present.

## Usage

### Quick Start

```sh
# Build the demo; make sure you have a RISC-V compiler
cd ./test_resources && make

# Run a simple program
cargo run -- ./bin/main.elf

# Run with debugger enabled
cargo run -- ./bin/main.elf -g
```

### Useful Command Line Options

- `-h`: Show help
- `-g`: Enable rvdb, the simple debugger (use the `help` command in rvdb for details)
- `-G`: Enable the GDB stub (listens on localhost:1234)
- `--device <TYPE:PATH>`: Configure a device
  - Example: `--device=virtio-block:/path/to/image`
- `<EXECUTABLE>`: Path to the binary/ELF executable file
- `--loglevel <LEVEL>`: Set log level

### Example Usage

```sh
cargo run -- ./test_resources/bin/virtio_blk_test.elf --device=virtio-block:./tmp/img_blk -g --loglevel=debug
```

### Running Linux

At present, the emulator can boot the Linux 6.18.2 kernel with BusyBox v1.37.0 in an initramfs via OpenSBI. You need to compile OpenSBI, the kernel, and BusyBox yourself, and adjust some configuration because RV64C is not yet supported. The `Makefile` in the repository root may be helpful.

## Virt Board

### MMIO Address Map

| Device | Address Base | Address Length |
| :-: | :-: | :-: |
| `power-manager`   | 0x0010_0000   | 0x1000    |
| `uart`            | 0x1000_0000   | 0x08      |
| `clint`           | 0x0200_0000   | 0x10000   |
| `virtio`          | 0x1000_1000   | 0x1000    |
| `ram`             | 0x8000_0000   | 0x800_0000|

## License

This project is licensed under the MIT License.

---

The "RISC-V" trade name is a registered trademark of RISC-V International. This project is not affiliated with, endorsed by, or sponsored by RISC-V International. For more information about RISC-V, please see [https://riscv.org](https://riscv.org).
