use crate::{config::arch_config::WordType, ram::Ram, ram_config::BASE_ADDR};

#[allow(unused)]
fn load_elf(ram: &mut Ram, elf_data: &[u8]) {
    let elf = xmas_elf::ElfFile::new(elf_data).unwrap();
    let elf_header = elf.header;
    let magic = elf_header.pt1.magic;
    assert_eq!(magic, [0x7f, 0x45, 0x4c, 0x46], "invalid elf!");
    let ph_count = elf_header.pt2.ph_count();
    for i in 0..ph_count {
        // read the i-th item form program header table.
        let ph = elf.program_header(i).unwrap();

        if ph.get_type().unwrap() == xmas_elf::program::Type::Load {
            let start_addr = (ph.virtual_addr() as usize) as WordType;
            let end_addr = ((ph.virtual_addr() + ph.mem_size()) as usize) as WordType;
            ram.insert_section(
                &elf.input[ph.offset() as usize..(ph.offset() + ph.file_size()) as usize],
                start_addr,
            );
        }
    }
}

#[allow(unused)]
fn load_bin(ram: &mut Ram, raw_data: &[u8]) {
    ram.insert_section(raw_data, BASE_ADDR);
}
