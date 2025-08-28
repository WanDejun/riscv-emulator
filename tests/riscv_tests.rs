#![allow(unused)]

use std::fs;
use std::path::{Path, PathBuf};

use riscv_emulator::Emulator;
use riscv_emulator::config::arch_config::WordType;
use riscv_emulator::isa::DebugTarget;
use riscv_emulator::load::get_section_addr;

fn get_test_by_name(name: &str) -> PathBuf {
    let isa_dir = Path::new("riscv-tests/isa");
    let test_path = isa_dir.join(name);
    test_path
}

fn find_tests(prefix: &str) -> Vec<PathBuf> {
    find_tests_exclude(prefix, &[])
}

fn find_tests_exclude(prefix: &str, exclude_names: &[&str]) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let isa_dir = Path::new("riscv-tests/isa");
    if let Ok(entries) = fs::read_dir(isa_dir) {
        for e in entries.flatten() {
            if e.path().is_dir() || e.path().extension() != None {
                continue;
            }

            if let Ok(fname) = e.file_name().into_string() {
                if fname.starts_with(prefix)
                    && exclude_names
                        .iter()
                        .all(|&n| (prefix.to_owned() + n) != fname)
                {
                    paths.push(e.path());
                }
            }
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

#[must_use]
fn run_test(elf: &Path) -> bool {
    // Load the ELF file and run it
    let result = std::panic::catch_unwind(|| {
        let mut run_result = false;
        let mut emu = Emulator::from_elf(&elf);
        let bytes = std::fs::read(elf).unwrap();
        let tohost: WordType = get_section_addr(&bytes, ".tohost").unwrap();

        emu.run_until(|cpu, instr_cnt| {
            // Handle tohost
            if (instr_cnt & (0xFFF)) == 0 {
                let msg = cpu.read_mem::<u64>(tohost).unwrap();

                if msg != 0 {
                    run_result = msg == 1;
                    if msg != 1 {
                        eprintln!("Test {:?} finished with message: {}", elf, msg);
                    }
                    return true;
                }
            }

            false
        })
        .unwrap();

        run_result
    });

    match result {
        Err(e) => {
            eprintln!("Test {:?} panicked: {:?}", elf, e);
            false
        }

        Ok(false) => {
            eprintln!("Test {:?} failed", elf);
            false
        }

        Ok(true) => {
            eprintln!("Test {:?} passed", elf);
            true
        }
    }
}

fn run_test_group(name: &str) {
    run_test_group_exclude(name, &[])
}

fn run_test_group_exclude(name: &str, exclude_names: &[&str]) {
    let tests = find_tests_exclude(name, exclude_names);
    assert!(
        !tests.is_empty(),
        "No tests named {} found in riscv-tests/isa",
        name
    );

    let tot = tests.len();

    let mut fail_cnt = 0;
    for elf in tests {
        fail_cnt += !run_test(&elf) as u32;
    }

    if fail_cnt > 0 {
        println!("Totally {}/{} tests failed in {}.", fail_cnt, tot, name);
    } else {
        println!("All tests passed in {}.", name);
    }

    assert!(fail_cnt == 0);
}

#[test]
#[cfg(feature = "riscv64")]
#[cfg(feature = "riscv-tests")]
fn run_rv64ui_p_tests() {
    run_test_group_exclude("rv64ui-p-", &["fence_i", "ma_data"]);
}

// #[test]
// #[cfg(feature = "riscv64")]
// #[cfg(feature = "riscv-tests")]
// fn run_rv64mi_p_tests() {
//     run_test_group_exclude("rv64mi-p-", &["breakpoint", "illegal"]);
// }

#[test]
#[cfg(feature = "riscv64")]
#[cfg(feature = "riscv-tests")]
fn run_rv32uf_p_tests() {
    run_test_group_exclude("rv64uf-p-", &[]);
}
