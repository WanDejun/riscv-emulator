//! Integration tests for the [`riscv-tests`] repo. Need feature `riscv-tests`.
//! You MUST compile the tests before using this, checkout the repo.
//!
//! [`riscv-tests`]: https://github.com/riscv-software-src/riscv-tests

#![allow(unused)]

use std::fs;
use std::path::{Path, PathBuf};

use crossterm::style::Stylize;
use riscv_emulator::Emulator;
use riscv_emulator::config::arch_config::WordType;
use riscv_emulator::isa::DebugTarget;
use riscv_emulator::isa::riscv::debugger::Address;
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
        let mut timeout = false;
        let mut run_result = false;
        let mut emu = Emulator::from_elf(&elf);
        let bytes = std::fs::read(elf).unwrap();
        let tohost: WordType = get_section_addr(&bytes, ".tohost").unwrap();

        emu.run_until(&mut |cpu, instr_cnt| {
            // Handle tohost
            if (instr_cnt & (0xFFF)) == 0 {
                let msg = cpu.read_memory::<u64>(Address::Phys(tohost)).unwrap();

                if msg != 0 {
                    run_result = msg == 1;
                    // if msg != 1 {
                    //     eprintln!("Test {:?} finished with message: {}", elf, msg);
                    // }
                    return true;
                }
            }

            if instr_cnt > 100_000 {
                timeout = true;
                return true;
            }

            false
        })
        .unwrap();

        (run_result, timeout)
    });

    let width = 48;

    match result {
        Err(e) => {
            eprintln!(
                "Test {:<width$}{}: {:?}",
                elf.display(),
                "panicked".red(),
                e
            );
            false
        }

        Ok((false, timeout)) => {
            if timeout {
                eprintln!("Test {:<width$}{}", elf.display(), "timedout".red());
            } else {
                eprintln!("Test {:<width$}{}", elf.display(), "failed".red());
            }
            false
        }

        Ok((true, _)) => {
            eprintln!("Test {:<width$}{}", elf.display(), "passed".green());
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
    run_test_group_exclude("rv64ui-p-", &["ma_data"]);
}

#[test]
#[cfg(feature = "riscv64")]
#[cfg(feature = "riscv-tests")]
fn run_rv64ui_v_tests() {
    run_test_group_exclude("rv64ui-v-", &["ma_data"]);
}

#[test]
#[cfg(feature = "riscv64")]
#[cfg(feature = "riscv-tests")]
fn run_rv64um_p_tests() {
    run_test_group("rv64um-p-");
}

#[test]
#[cfg(feature = "riscv64")]
#[cfg(feature = "riscv-tests")]
fn run_rv64mi_p_tests() {
    run_test_group_exclude("rv64mi-p-", &["pmpaddr", "sbreak", "breakpoint"]);
}

#[test]
#[cfg(feature = "riscv64")]
#[cfg(feature = "riscv-tests")]
fn run_rv64uf_p_tests() {
    run_test_group("rv64uf-p-");
}

#[test]
#[cfg(feature = "riscv64")]
#[cfg(feature = "riscv-tests")]
fn run_rv64si_p_tests() {
    run_test_group_exclude("rv64si-p-", &["sbreak"]);
}
