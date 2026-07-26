# RISC-V Emulator

## About

`RISC-V Emulator` is an educational full-system emulator for the RISC-V architecture, written in Rust :fire:.

The main features of `RISC-V Emulator` include:

- Supported ISA:
  - RV64GC (RV64IMAFDC, Zicsr, Zifencei)
  - Partial `V` extension support (vector floating-point instructions are still incomplete)
- Supported privilege modes:
  - M, S, and U modes
- A simple debugger monitor called rvdb
- GDB support
- Virtual memory
- Devices:
  - CLINT, PLIC, serial, and VirtIO MMIO (block devices only at present)

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

We use the [riscv-tests](https://github.com/riscv-software-src/riscv-tests) submodule as our test suite. Initialize the submodule, install [riscv-gnu-toolchain](https://github.com/riscv-collab/riscv-gnu-toolchain), and follow the riscv-tests README to build its ISA binaries:

```sh
git submodule update --init --recursive
```

After building the riscv-tests binaries, run:

```sh
cargo test --features riscv-tests
```

Test support for `riscv-arch-test` also exists, but it is not integrated into CI. Unfortunately, the test suite stabilized at 4.x a few months after we implemented support for 3.x, so the suite we use is not up to date at present.

## Usage

### Quick Start

```sh
# Build the demo; make sure you have a RISC-V compiler
make -C ./test_resources

# Run a simple program
cargo run -- ./test_resources/bin/main.elf

# Run with debugger enabled
cargo run -- ./test_resources/bin/main.elf -g
```

### Useful Command-Line Options

- `<PATH>`: RISC-V ELF executable or raw binary image to run
- `-h, --help`: Print help
- `-f, --format <auto|elf|bin>`: Choose the input format; `auto` uses the `.elf` or `.bin` filename extension
- `-g, --debug`: Start the built-in rvdb debugger (run `help` inside rvdb for its command list)
- `-G, --gdb`: Start a GDB remote stub on `localhost:1234`
- `-S, --script <FILE>`: Run rvdb commands from a file before entering the interactive debugger; requires `--debug`
- `-v, --verbose`: Print additional startup details
- `--loglevel <LEVEL>`: Set the logging level (`trace`, `debug`, `info`, `warn`, or `error`)
- `--device <TYPE:PATH>`: Attach a VirtIO block device; may be repeated
  - Use `virtio-block:/path/to/image`; the image must exist and its size must be a multiple of 512 bytes
- `--isa <ISA>`: Configure the decoder with an ISA string (default: `RV64GC`)
- `--max-cycles <COUNT>`: Stop after the requested number of emulated cycles (`0` disables the limit)
- `--dtb <FILE>`: Load a DTB and pass its guest address to OpenSBI in register `a1`
- `--dtb-address <ADDRESS>`: Set the guest physical address for `--dtb` (default: `0x9f000000`)

During normal emulation, press `Ctrl+A`, release the keys, and then press `x` to exit emulator.

### Example Usage

```sh
mkdir -p ./tmp
truncate -s 4K ./tmp/img_blk
cargo run -- ./test_resources/bin/virtio_blk_test.elf --device=virtio-block:./tmp/img_blk --loglevel=debug
```

### Running Linux

At present, the emulator can boot the Linux 6.18.2 kernel with BusyBox v1.37.0 in an initramfs via OpenSBI. Configure the kernel with the required BusyBox initramfs, then provide Linux and OpenSBI source trees to the root `Makefile`:

```sh
make linux LINUX_DIR=/path/to/linux OPENSBI_DIR=/path/to/opensbi
```

The target builds `dts/virt.dtb`, the kernel image, and the OpenSBI payload before starting the emulator. `CROSS_COMPILE`, `PLATFORM_RISCV_ISA`, and other build variables can be overridden when necessary.

#### Use a VirtIO Device in Linux

Enable the `virtio_mmio@10001000` node in `dts/virt.dts`, create a sector-aligned backing image, and pass the device through `RVEMU_ARGS`:

```sh
mkdir -p ./tmp
truncate -s 4K ./tmp/img_blk
make linux LINUX_DIR=/path/to/linux OPENSBI_DIR=/path/to/opensbi \
  RVEMU_ARGS='--device=virtio-block:./tmp/img_blk'
```

When Linux boots, the kernel log will show the device being recognized:

```
[   72.993894] virtio_blk virtio0: 1/0/0 default/read/poll queues
[   73.013248] virtio_blk virtio0: [vda] 8 512-byte logical blocks (4.10 kB/4.00 KiB)
```

If devtmpfs has not already created `/dev/vda`, create it from the Linux shell:

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
  dd if=/dev/vda bs=512 count=8 2>/dev/null | hexdump -C
  echo "VirtIO-Blk Write Test Success!" | dd of=/dev/vda bs=512 count=1 conv=notrunc
  ```
- **Create a filesystem and mount it** (this overwrites existing data in the backing image):
  ```sh
  mkfs.ext2 /dev/vda
  mount /dev/vda /mnt
  ```

Additional device metadata is available under `/sys/block/vda/`.

## Virt Board

### MMIO Address Map

|       Device       | Address Base |    Address Length    | PLIC Interrupt ID |
| :----------------: | :----------: | :------------------: | :---------------: |
|  `power-manager`   | 0x0010_0000  |        0x1000        |         -         |
|   `test-device`*   | 0x0010_1000  |         0x10         |       0x3f        |
|      `clint`       | 0x0200_0000  |       0x10000        |         -         |
|       `plic`       | 0x0c00_0000  |      0x0400_0000     |         -         |
|       `uart`       | 0x1000_0000  |         0x08         |       0x0a        |
| `virtio-mmio[0]`** | 0x1000_1000  |        0x1000        |       0x01        |
|       `ram`        | 0x8000_0000  | 0x2000_0000 (512 MiB) |         -         |

\* `test-device` is mapped when the `test-device` Cargo feature is enabled; it is part of the default feature set.

\** Additional VirtIO MMIO transports use consecutive 0x1000-byte regions and interrupt IDs. Keep assigned IDs distinct from the UART interrupt ID 0x0a.

## License

This project is licensed under the MIT License.

---

The "RISC-V" trade name is a registered trademark of RISC-V International. This project is not affiliated with, endorsed by, or sponsored by RISC-V International. For more information about RISC-V, please see [https://riscv.org](https://riscv.org).
