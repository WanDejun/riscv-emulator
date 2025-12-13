use std::fs;
use std::path::PathBuf;

use criterion::{Criterion, black_box, criterion_group, criterion_main};

use riscv_emulator::Emulator;

fn bench_emulator_run(c: &mut Criterion) {
    let mut group = c.benchmark_group("emulator_run");
    group.sample_size(50);

    let bin_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test_resources/bin");
    let entries = fs::read_dir(&bin_dir).expect("Failed to read bench ELF dir.");

    const IGNORE: [&str; 1] = ["virtio_blk_test"];

    for e in entries {
        if let Ok(entry) = e {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("elf") {
                continue;
            }

            if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                if IGNORE.contains(&name) {
                    continue;
                }

                let bench_name = format!("load_and_run_{}", name);
                group.bench_function(&bench_name, move |b| {
                    b.iter(|| {
                        let mut emu = Emulator::from_elf(&path);
                        black_box(emu.run().unwrap());
                    })
                });
            }
        }
    }

    group.finish();
}

criterion_group!(benches, bench_emulator_run);
criterion_main!(benches);
