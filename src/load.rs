use xmas_elf::symbol_table::Entry;

use crate::{config::arch_config::WordType, ram::Ram, ram_config::BASE_ADDR, utils::BiMap};

pub struct SymTab {
    pub func_table: BiMap<String, u64>,
}

impl SymTab {
    pub fn func_addr_by_name(&self, name: &str) -> Option<u64> {
        self.func_table.get_by_left(&name.to_string()).cloned()
    }

    pub fn func_name_by_addr(&self, addr: u64) -> Option<&String> {
        self.func_table.get_by_right(&addr)
    }
}

pub struct ELFLoader {
    elf_data: Vec<u8>,
}

impl ELFLoader {
    pub fn new(elf_data: Vec<u8>) -> ELFLoader {
        ELFLoader { elf_data }
    }

    fn elf(&'_ self) -> xmas_elf::ElfFile<'_> {
        xmas_elf::ElfFile::new(&self.elf_data).unwrap()
    }

    pub fn load_to_ram(&self, ram: &mut Ram) {
        let elf = self.elf();
        for ph in elf.program_iter() {
            if ph.get_type().unwrap() == xmas_elf::program::Type::Load {
                let start_addr = (ph.virtual_addr() as usize) as WordType;
                // let end_addr = ((ph.virtual_addr() + ph.mem_size()) as usize) as WordType;

                ram.insert_section(
                    &elf.input[ph.offset() as usize..(ph.offset() + ph.file_size()) as usize],
                    start_addr - BASE_ADDR,
                );
            }
        }
    }

    pub fn get_section_addr(&self, section_name: &str) -> Option<WordType> {
        let elf = self.elf();
        for sh in elf.section_iter() {
            if let Ok(name) = sh.get_name(&elf) {
                if name == section_name {
                    let addr = sh.address();
                    return Some(addr as WordType);
                }
            }
        }

        None
    }

    fn parse_symtab(&self, entries: &[xmas_elf::symbol_table::Entry64]) -> SymTab {
        let mut func_table = BiMap::<String, u64>::new();

        for entry in entries {
            // if entry.get_type().unwrap() == xmas_elf::symbol_table::Type::Func {
            let name = entry.get_name(&self.elf()).unwrap().to_string();
            let addr = entry.value();
            func_table.insert(name, addr);
            // }
        }

        SymTab { func_table }
    }

    pub fn get_symbol_table(&self) -> Option<SymTab> {
        let elf = self.elf();
        for sh in elf.section_iter() {
            if let Ok(name) = sh.get_name(&elf) {
                if name == ".symtab" {
                    // TODO: Handle 32 bit ELF files
                    if let xmas_elf::sections::SectionData::SymbolTable64(symtab) =
                        sh.get_data(&elf).unwrap()
                    {
                        return Some(self.parse_symtab(&symtab));
                    } else {
                        unreachable!();
                    }
                }
            }
        }

        None
    }
}

pub fn load_bin(ram: &mut Ram, raw_data: &[u8]) {
    ram.insert_section(raw_data, 0);
}
