OUTPUT_ARCH(riscv)
ENTRY(_start)
BASE_ADDRESS = 0x80000000;

SECTIONS {
    . = BASE_ADDRESS;
    startkernel = .;

    starttext = .;
    .text : {
        *(.text.ENTRY)
        . = ALIGN(4K);
        *(.text)
    }
    . = ALIGN(4K);
    endtext = .;


    startrodata = .;
    .rodata : {
        *(.rodata .rodata.*)
        *(.srodata ..srodata.*)
    }
    . = ALIGN(4K);
    endrodata = .;


    startdata = .;
    .data : {
        *(.data .data.*)
        *(.sdata .sdata.*)
    }
    . = ALIGN(4K);
    enddata = .;


    startbss = .;
    .bss : {
        sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
    }
    . = ALIGN(4K);
    endbss = .;

    .stack : {
        *(.stack)
    }

    . = ALIGN(4K);
    endkernel = .;

    . = ALIGN(4K);
    /DISCARD/ : {
        *(.eh_frame)
    }
}