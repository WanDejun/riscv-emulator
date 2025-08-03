ARCH := riscv64
SRC := ./src
INC := ./include
LIB := ./lib
OBJ := ./obj
BIN := ./bin
CC := $(ARCH)-elf-gcc
C_FLAG := -ffreestanding -nostdlib -march=rv64gc -mabi=lp64d -mcmodel=medany -I$(INC) -g
LD := $(ARCH)-elf-ld
LD_FLAG := -static -g
QEMU := qemu-system-$(ARCH)

TARGET := main

.PNONY:
	build_lib build run clear 


$(OBJ)/%.o: $(LIB)/%.c
	@ mkdir $(OBJ) $(BIN) -p
	$(CC) $(C_FLAG) -c $^ -o $@

build_lib: $(OBJ)/io.o $(OBJ)/log.o

build: $(SRC)/boot.S $(SRC)/$(TARGET).c linker.ld build_lib
	@ $(CC) $(C_FLAG) -c $(SRC)/$(TARGET).c -o $(OBJ)/$(TARGET).o
	@ $(CC) $(C_FLAG) -c $(SRC)/boot.S -o $(OBJ)/boot.o
	@ $(LD) $(LD_FLAG) -T linker.ld -o $(BIN)/$(TARGET).elf $(OBJ)/$(TARGET).o $(OBJ)/boot.o $(OBJ)/io.o $(OBJ)/log.o

run: build
	$(QEMU) -machine virt -bios $(BIN)/$(TARGET).elf -serial stdio

clear:
	rm -r $(BIN) $(OBJ)