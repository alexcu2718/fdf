use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use fdf::access_dirent;

use core::num::NonZeroU64;
use std::hint::black_box;

#[inline]
//modified version to work for this test function(copy pasted really)
pub const unsafe fn dirent_const_time_strlen(dirent: *const LibcDirent64) -> usize {
    const DIRENT_HEADER_START: usize = core::mem::offset_of!(LibcDirent64, d_name);
    const MINIMUM_DIRENT_SIZE: usize = DIRENT_HEADER_START.next_multiple_of(8);
    const LO_U64: u64 = u64::from_ne_bytes([0x01; size_of::<u64>()]);
    const HI_U64: u64 = u64::from_ne_bytes([0x80; size_of::<u64>()]);
    let reclen = unsafe { (*dirent).d_reclen } as usize;
    /*
      Read the last 8 bytes of the struct as a u64.
    This works because dirents are always 8-byte aligned. */
    // SAFETY: We're indexing in bounds within the pointer (it is guaranteed aligned by the kernel)
    let last_word: u64 = unsafe { *(dirent.byte_add(reclen - 8).cast::<u64>()) };

    const MASK: u64 = u64::from_ne_bytes([0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00]);

    let mask: u64 = MASK * ((reclen == MINIMUM_DIRENT_SIZE) as u64);

    let candidate_pos: u64 = last_word | mask;

    let zero_bit = unsafe {
        NonZeroU64::new_unchecked(candidate_pos.wrapping_sub(LO_U64) & !candidate_pos & HI_U64)
    };
    // Find the position of the null terminator
    #[cfg(target_endian = "little")]
    let byte_pos = (zero_bit.trailing_zeros() >> 3) as usize;
    #[cfg(target_endian = "big")]
    let byte_pos = (zero_bit.leading_zeros() >> 3) as usize;

    reclen - DIRENT_HEADER_START + byte_pos - 8
}

#[repr(C)]
pub struct LibcDirent64 {
    // Fake a structure similar to libc::dirent64
    pub d_ino: u64,
    pub d_off: u64,
    pub d_reclen: u16,
    pub d_type: u8,
    pub d_name: [u8; 256],
}

const fn calculate_min_reclen(name_len: usize) -> u16 {
    const HEADER_SIZE: usize = std::mem::offset_of!(LibcDirent64, d_name);
    let total_size = HEADER_SIZE + name_len + 1;
    total_size.next_multiple_of(8) as _
    //reclen follows specification: must be multiple of 8 and at least 24 bytes but we calculate the reclen based on the name length
    //this works because it's given the same representation in memory so repr C will ensure the layout is compatible
}

fn make_dirent(name: &str) -> LibcDirent64 {
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

    entry
}

fn bench_strlen(c: &mut Criterion) {
    let length_groups = [
        ("tiny (1-4)", "a"),
        ("small (5-16)", "file.txtth6"),
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
                |b, e| {
                    b.iter(|| unsafe {
                        black_box(dirent_const_time_strlen(black_box(e as *const _)))
                    })
                },
            );

            group.bench_with_input(
                BenchmarkId::new("libc_strlen", size_name),
                &entry,
                |b, e| {
                    b.iter(|| unsafe {
                        black_box(libc::strlen(black_box(access_dirent!(e, d_name))))
                    })
                },
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
                    total += unsafe {
                        black_box(dirent_const_time_strlen(black_box(entry as *const _)))
                    };
                }
                black_box(total) //make sure compiler does not optimise this away
            })
        });

        batch_group.bench_function("libc_strlen_batch", |b| {
            b.iter(|| {
                let mut total = 0;
                for entry in &all_entries {
                    total += unsafe {
                        black_box(libc::strlen(black_box(access_dirent!(entry, d_name))))
                    };
                }
                black_box(total) //make sure compiler does not optimise this away
            })
        });

        batch_group.finish();
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(10000)
        .warm_up_time(std::time::Duration::from_millis(500))
        .measurement_time(std::time::Duration::from_secs(2));
    targets = bench_strlen
}

criterion_main!(benches);
