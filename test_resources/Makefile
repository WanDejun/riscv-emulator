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
TARGET_ELFS := $(patsubst %,$(BIN)/%.elf,$(TARGETS))

LIBS := io log boot
LIBS_OBJ := $(patsubst %,$(OBJ)/%.o,$(LIBS))

# proctect intermediate file.
.PRECIOUS: $(OBJ)/%.o

.PHONY: build_lib build run clean build_all
DEFAULT_TARGET: build_all

build_all: $(TARGET_ELFS)

# default lib file.
$(OBJ)/%.o: $(LIB)/%.c
	@ mkdir $(OBJ) $(BIN) -p
	$(CC) $(C_FLAG) -c $^ -o $@

# boot.o
$(OBJ)/boot.o: $(SRC)/boot.S
	@ mkdir -p $(OBJ) $(BIN)
	$(CC) $(C_FLAG) -c $< -o $@


build: $(LIBS_OBJ) $(BIN)/$(TARGET).elf 

$(BIN)/%.elf: $(OBJ)/%.o $(LIBS_OBJ) linker.ld
	@ mkdir -p $(OBJ) $(BIN)
	$(LD) $(LD_FLAG) -T linker.ld -o $@ $(filter %.o,$^)
	
# default .c file
$(OBJ)/%.o: $(SRC)/%.c
	@ mkdir -p $(OBJ) $(BIN)
	$(CC) $(C_FLAG) -c $< -o $@


run: build
	$(QEMU) -machine virt -bios $(BIN)/$(TARGET).elf -serial stdio

clean:
	rm -r $(BIN) $(OBJ)