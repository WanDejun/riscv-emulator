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

TARGETS := main fib prime matrix_mul io_bench trap_test
TARGET_ELFS := $(patsubst %,$(BIN)/%.elf,$(TARGETS))

LIBS := io log boot trap
LIBS_OBJ := $(patsubst %,$(OBJ)/%.o,$(LIBS))


$(OBJ)/%.o: $(LIB)/%.c
	@ mkdir $(OBJ) $(BIN) -p
	$(CC) $(C_FLAG) -c $^ -o $@

# boot.o
$(OBJ)/boot.o: $(SRC)/boot.S
	@ mkdir -p $(OBJ) $(BIN)
	$(CC) $(C_FLAG) -c $< -o $@

# trap.o
$(OBJ)/trap.o: $(SRC)/trap.S
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

build: $(SRC)/boot.S $(SRC)/$(TARGET).c linker.ld build_lib
	@ $(CC) $(C_FLAG) -c $(SRC)/$(TARGET).c -o $(OBJ)/$(TARGET).o
	@ $(CC) $(C_FLAG) -c $(SRC)/boot.S -o $(OBJ)/boot.o
	@ $(LD) $(LD_FLAG) -T linker.ld -o $(BIN)/$(TARGET).elf $(OBJ)/$(TARGET).o $(OBJ)/boot.o $(OBJ)/io.o $(OBJ)/log.o

run: build
	$(QEMU) -machine virt -bios $(BIN)/$(TARGET).elf -serial stdio

clear:
	rm -r $(BIN) $(OBJ)