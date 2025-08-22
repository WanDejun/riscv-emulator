use std::fs;
use std::path::{Path, PathBuf};

use riscv_emulator::Emulator;
use riscv_emulator::config::arch_config::WordType;
use riscv_emulator::isa::DebugTarget;

#[allow(unused)]
fn get_test_by_name(name: &str) -> PathBuf {
    let isa_dir = Path::new("riscv-tests/isa");
    let test_path = isa_dir.join(name);
    test_path
}

fn find_tests(prefix: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let isa_dir = Path::new("riscv-tests/isa");
    if let Ok(entries) = fs::read_dir(isa_dir) {
        for e in entries.flatten() {
            if e.path().is_dir() || e.path().extension() != None {
                continue;
            }

            if let Ok(fname) = e.file_name().into_string() {
                if fname.starts_with(prefix) {
                    // keep full filename (may be an ELF with no extension)
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
    eprintln!("Running {:?}", elf);

    let result = std::panic::catch_unwind(|| {
        let mut run_result = false;
        let mut emu = Emulator::from_elf(&elf);
        emu.run_until(|cpu, instr_cnt| {
            // Handle tohost
            const TOHOST: WordType = 0x80001000;

            if (instr_cnt & (0xFFF)) == 0 {
                let msg = cpu.read_mem::<u64>(TOHOST).unwrap();

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

#[test]
#[cfg(feature = "riscv64")]
#[cfg(feature = "riscv-tests")]
fn run_all_rv64ui_p_tests() {
    let tests = find_tests("rv64ui-p-");
    assert!(
        !tests.is_empty(),
        "No rv64ui-p tests found in riscv-tests/isa"
    );

    let tot = tests.len();

    let mut fail_cnt = 0;
    for elf in tests {
        fail_cnt += !run_test(&elf) as u32;
    }

    println!("Totally {}/{} tests failed in rv64ui-p.", fail_cnt, tot);

    assert!(fail_cnt == 0);
}
