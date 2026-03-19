# Makefile for building and running linux kernel on RISC-V emulator (and on QEMU for debugging).

JOBS ?= 14
CROSS_COMPILE ?= riscv64-linux-gnu-
PLATFORM_RISCV_ISA ?= rv64g
FW_PAYLOAD_FDT_ADDR ?= 0xA2000000
RVEMU_ARGS ?=

EMU_DIR ?= $(CURDIR)
LINUX_DIR ?=
OPENSBI_DIR ?=

DTS_FILE ?= $(EMU_DIR)/dts/virt.dts
DTB_FILE ?= $(EMU_DIR)/dts/virt.dtb
LINUX_IMAGE ?= $(LINUX_DIR)/arch/riscv/boot/Image
FW_BIN ?= $(OPENSBI_DIR)/build/platform/generic/firmware/fw_payload.bin

.PHONY: check build-dtb build-linux build-opensbi linux-qemu linux linux-debug

check:
	@test -n "$(LINUX_DIR)" || (echo "error: LINUX_DIR is empty. set env LINUX_DIR=... or run make LINUX_DIR=..."; exit 1)
	@test -n "$(OPENSBI_DIR)" || (echo "error: OPENSBI_DIR is empty. set env OPENSBI_DIR=... or run make OPENSBI_DIR=..."; exit 1)
	@test -f "$(DTS_FILE)" || (echo "error: missing $(DTS_FILE)"; exit 1)
	@test -f "$(LINUX_DIR)/Makefile" || (echo "error: missing $(LINUX_DIR)/Makefile"; exit 1)
	@test -f "$(OPENSBI_DIR)/Makefile" || (echo "error: missing $(OPENSBI_DIR)/Makefile"; exit 1)

build-dtb: check
	dtc -I dts -O dtb -o "$(DTB_FILE)" "$(DTS_FILE)"

build-linux: check
	$(MAKE) -C "$(LINUX_DIR)" ARCH=riscv CROSS_COMPILE="$(CROSS_COMPILE)" Image -j"$(JOBS)"

build-opensbi: build-dtb build-linux
	@test -f "$(LINUX_IMAGE)" || (echo "error: missing $(LINUX_IMAGE)"; exit 1)
	$(MAKE) -C "$(OPENSBI_DIR)" PLATFORM=generic -j"$(JOBS)" \
		CROSS_COMPILE="$(CROSS_COMPILE)" \
		PLATFORM_RISCV_ISA="$(PLATFORM_RISCV_ISA)" \
		FW_PAYLOAD_PATH="$(LINUX_IMAGE)" \
		FW_FDT_PATH="$(DTB_FILE)" \
		FW_PAYLOAD_FDT_ADDR="$(FW_PAYLOAD_FDT_ADDR)"

linux-qemu: build-opensbi
	@test -f "$(FW_BIN)" || (echo "error: missing $(FW_BIN)"; exit 1)
	qemu-system-riscv64 -M virt -m 2G -nographic -bios "$(FW_BIN)"

linux: build-opensbi
	@test -f "$(FW_BIN)" || (echo "error: missing $(FW_BIN)"; exit 1)
	cargo run --release -- "$(FW_BIN)" $(RVEMU_ARGS)

linux-debug: build-opensbi
	@test -f "$(FW_BIN)" || (echo "error: missing $(FW_BIN)"; exit 1)
	cargo run --release -- "$(FW_BIN)" -g $(RVEMU_ARGS)
