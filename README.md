# RISC-V Emulator

## About

`RISC-V Emulator` is an educational full-system emulator for the RISC-V architecture, written in Rust :fire:.

The main features of `RISC-V Emulator` include:

- Supported ISA:
  - RV64GC (RV64IMAFDC, Zicsr, Zifencei)
  - supports partial `V` extensions (floating-point support is still not fully developed)
- Supported privilege modes:
  - M, S, and U modes
- A simple debugger monitor called rvdb
- GDB support
- Virtual memory
- Devices:
  - CLINT, PLIC, serial, and virtIO-mmio (Only support block device currently)

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

#### Use Virtio Device in Linux

When booting Linux with a Virtio block device attached, the kernel log will show the device being recognized:

```
[   72.993894] virtio_blk virtio0: 1/0/0 default/read/poll queues
[   73.013248] virtio_blk virtio0: [vda] 8 512-byte logical blocks (4.10 kB/4.00 KiB)
```

To access the Virtio block device (`/dev/vda`) from the Linux shell:

1. **Determine the device number** — read the major/minor numbers from sysfs:
   ```sh
   cat /sys/block/vda/dev
   ```
   This typically outputs `254:0`.

2. **Create the device node** — use `mknod` to create the block device file:
   ```sh
   mknod /dev/vda b 254 0
   ```

3. **Verify** — check that the device node appears:
   ```sh
   ls /dev
   ```

Once `/dev/vda` is available, you can perform block-level operations:

- **Read/write raw data** with `dd`:
  ```sh
  dd if=/dev/vda of=/tmp/output bs=512 count=8
  dd if=/tmp/data of=/dev/vda bs=512 count=4 seek=2
  ```
- **Create a filesystem and mount it** (if the device contains a disk image with a partition table or filesystem):
  ```sh
  mkfs.ext2 /dev/vda
  mount /dev/vda /mnt
  ```

Additional device metadata is available under `/sys/block/vda/`.

## Virt Board

### MMIO Address Map

|     Device      | Address Base |   Address Length    | Interrupt ID |
| :-------------: | :----------: | :-----------------: | :----------: |
| `power-manager` | 0x0010_0000  |       0x1000        |      -       |
|     `uart`      | 0x1000_0000  |        0x08         |     0x0a     |
|     `clint`     | 0x0200_0000  |       0x10000       |      -       |
|  `virtio-mmio`  | 0x1000_1000  |       0x1000        |     0x01     |
|      `ram`      | 0x8000_0000  | 0x2000_0000 (512MB) |      -       |

## License

This project is licensed under the MIT License.

---

The "RISC-V" trade name is a registered trademark of RISC-V International. This project is not affiliated with, endorsed by, or sponsored by RISC-V International. For more information about RISC-V, please see [https://riscv.org](https://riscv.org).
