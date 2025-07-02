#[cfg(target_os = "linux")]
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
#[cfg(target_os = "linux")]
use fdf::{dirent_const_time_strlen,strlen as asm_strlen};
#[cfg(target_os = "linux")]
use libc::{c_char, dirent64};
#[cfg(target_os = "linux")]
use std::hint::black_box;

#[cfg(target_os = "linux")]
#[repr(C, align(8))]
pub struct LibcDirent64 {
    // Fake a structure similar to libc::dirent64 which we transmute later
    pub d_ino: u64,
    pub d_off: u64,
    pub d_reclen: u16,
    pub d_type: u8,
    pub d_name: [u8; 256],
}
#[cfg(target_os = "linux")]
const fn calculate_min_reclen(name_len: usize) -> u16 {
    const HEADER_SIZE: usize = std::mem::offset_of!(LibcDirent64, d_name);
    let total_size = HEADER_SIZE + name_len + 1;
    ((total_size + 7) & !7) as u16 //reclen follows specification: must be multiple of 8 and at least 24 bytes but we calculate the reclen based on the name length
    //this works because it's given the same representation in memory so repr C will ensure the layout is compatible
}
#[cfg(target_os = "linux")]
fn make_dirent(name: &str) -> dirent64 {
    let bytes = name.as_bytes();
    assert!(bytes.len() < 256, "Name too long for dirent structure");

    let min_reclen = calculate_min_reclen(bytes.len());
    let mut entry = LibcDirent64 {
        d_ino: 0,
        d_off: 0,
        d_reclen: min_reclen,
        d_type: 0,
        d_name: [0; 256],
    };

    entry.d_name[..bytes.len()].copy_from_slice(bytes);
    entry.d_name[bytes.len()] = 0;

    unsafe { std::mem::transmute(entry) }
}
#[cfg(target_os = "linux")]
fn bench_strlen(c: &mut Criterion) {
    // First create all test cases
    let length_groups = [
        ("tiny (1-4)", "a"),
        ("small (5-16)", "file.txt"),
        ("medium (17-64)", "config_files/settings/default.json"),
        (
            "large (65-128)",
            "very_long_directory_name/with_subfolders/and_a_very_long_filename.txt",
        ),
        ("xlarge (129-255)", &"a".repeat(200)),
        ("max length (255)", &"b".repeat(255)),
    ];

    let all_entries: Vec<_> = length_groups
        .iter()
        .map(|(_, name)| make_dirent(name))
        .collect();

    //  make separate benchmark groups one at a time
    {
        let mut group = c.benchmark_group("strlen_by_length");

        for (size_name, name) in length_groups {
            let entry = make_dirent(name);
            let byte_len = name.len();

            group.throughput(Throughput::Bytes(byte_len as u64));

            group.bench_with_input(
                BenchmarkId::new("const_time_swar", size_name),
                &entry,
                |b, e| b.iter(|| unsafe { black_box(dirent_const_time_strlen(black_box(e))) }),
            );

            group.bench_with_input(
                BenchmarkId::new("libc_strlen", size_name),
                &entry,
                |b, e| {
                    b.iter(|| unsafe {
                        black_box(libc::strlen(black_box(e.d_name.as_ptr() as *const c_char)))
                    })
                },
            );

            group.bench_with_input(
                BenchmarkId::new("asm_strlen", size_name),
                &entry,
                |b, e| b.iter(|| unsafe { black_box(asm_strlen(black_box(e.d_name.as_ptr() as *const c_char))) }),
            );
        }
        group.finish();
    }

    //  create the batch comparison group
    {
        let mut batch_group = c.benchmark_group("strlen_batch_comparison");
        batch_group.throughput(Throughput::Elements(all_entries.len() as u64));

        batch_group.bench_function("const_time_swar_batch", |b| {
            b.iter(|| {
                let mut total = 0;
                for entry in &all_entries {
                    total += unsafe { black_box(dirent_const_time_strlen(black_box(entry))) };
                }
                black_box(total)
            })
        });

        batch_group.bench_function("libc_strlen_batch", |b| {
            b.iter(|| {
                let mut total = 0;
                for entry in &all_entries {
                    total += unsafe {
                        black_box(libc::strlen(black_box(
                            entry.d_name.as_ptr() as *const c_char
                        )))
                    };
                }
                black_box(total)
            })
        });
        batch_group.bench_function("asm_strlen_batch", |b| {
            b.iter(|| {
                let mut total = 0;
                for entry in &all_entries {
                    total += unsafe { black_box(asm_strlen(black_box(entry.d_name.as_ptr() as *const c_char))) };
                }
                black_box(total)
            })
        });

        batch_group.finish();
    }
}

#[cfg(target_os = "linux")]
criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(1000)
        .warm_up_time(std::time::Duration::from_millis(500))
        .measurement_time(std::time::Duration::from_secs(3));
    targets = bench_strlen
}
#[cfg(target_os = "linux")]
criterion_main!(benches);

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!(
        "
        THIS TEST IS ONLY VALID FOR LINUX IGNORE"
    )
}
