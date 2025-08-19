ARCH := riscv64
SRC := ./src
INC := ./include
LIB := ./lib
OBJ := ./obj
BIN := ./bin
CC := $(ARCH)-elf-gcc
C_FLAG := -ffreestanding -nostdlib -march=rv64im -mabi=lp64 -mcmodel=medany -I$(INC) -g
LD := $(ARCH)-elf-ld
LD_FLAG := -static -g
QEMU := qemu-system-$(ARCH)

TARGET ?= main

TARGETS := main fib prime matrix_mul io_bench

DEFAULT_TARGET: build_all

.PHONY: build_lib build run clean build_all

build_all:
	@for t in $(TARGETS); do \
		$(MAKE) -s build TARGET=$$t || exit $$?; \
	done


$(OBJ)/%.o: $(LIB)/%.c
	@ mkdir $(OBJ) $(BIN) -p
	$(CC) $(C_FLAG) -c $^ -o $@

build_lib: $(OBJ)/io.o $(OBJ)/log.o

build: $(SRC)/boot.S $(SRC)/$(TARGET).c linker.ld build_lib
	@ mkdir -p $(OBJ) $(BIN)
	@ $(CC) $(C_FLAG) -c $(SRC)/$(TARGET).c -o $(OBJ)/$(TARGET).o
	@ $(CC) $(C_FLAG) -c $(SRC)/boot.S -o $(OBJ)/boot.o
	@ $(LD) $(LD_FLAG) -T linker.ld -o $(BIN)/$(TARGET).elf $(OBJ)/$(TARGET).o $(OBJ)/boot.o $(OBJ)/io.o $(OBJ)/log.o

run: build
	$(QEMU) -machine virt -bios $(BIN)/$(TARGET).elf -serial stdio

clean:
	rm -r $(BIN) $(OBJ)