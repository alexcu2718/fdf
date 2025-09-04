use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

use fdf::strlen as asm_strlen;
use std::hint::black_box;

#[inline(always)]
//modified version to work for this test function(copy pasted really)
pub const unsafe fn dirent_const_time_strlen(dirent: *const LibcDirent64) -> usize {
    const DIRENT_HEADER_START: usize = std::mem::offset_of!(LibcDirent64, d_name) + 1; //we're going backwards(to the start of d_name) so we add 1 to the offset
    let reclen = unsafe { (*dirent).d_reclen } as usize; //(do not access it via byte_offset!)
    // Calculate find the  start of the d_name field
    //  Access the last 8 bytes(word) of the dirent structure as a u64 word
    #[cfg(target_endian = "little")]
    let last_word = unsafe { *((dirent as *const u8).add(reclen - 8) as *const u64) }; //DO NOT USE BYTE OFFSET.
    #[cfg(target_endian = "big")]
    let last_word = unsafe { *((dirent as *const u8).add(reclen - 8) as *const u64) }.to_le(); // Convert to little-endian if necessary

    let mask = 0x00FF_FFFFu64 * ((reclen == 24) as u64); // (multiply by 0 or 1)

    let candidate_pos = last_word | mask;

    let byte_pos = 7 - find_zero_byte_u64(candidate_pos);

    reclen - DIRENT_HEADER_START - byte_pos
}

//repeated definitions (because i had to make find_zero_byte_u64 private)
#[inline]
pub(crate) const fn repeat_u64(byte: u8) -> u64 {
    u64::from_ne_bytes([byte; size_of::<u64>()])
}

const LO_U64: u64 = repeat_u64(0x01);

const HI_U64: u64 = repeat_u64(0x80);

#[inline]
pub(crate) const fn find_zero_byte_u64(x: u64) -> usize {
    //use the same trick seen earlier, except this time we have to use  hardcoded u64 values  to find the position of the 0 bit
    let zero_bit = x.wrapping_sub(LO_U64) & !x & HI_U64;

    (zero_bit.trailing_zeros() >> 3) as usize
    //>> 3 converts from bit position to byte index (divides by 8)
}

#[repr(C, align(8))]
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
    (((total_size + 7) / 8) * 8) as u16 //reclen follows specification: must be multiple of 8 and at least 24 bytes but we calculate the reclen based on the name length
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
                        black_box(libc::strlen(black_box(e.d_name.as_ptr() as *const _)))
                    })
                },
            );

            group.bench_with_input(BenchmarkId::new("asm_strlen", size_name), &entry, |b, e| {
                b.iter(|| unsafe {
                    black_box(asm_strlen(black_box(e.d_name.as_ptr() as *const _)))
                })
            });
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
                        black_box(libc::strlen(black_box(entry.d_name.as_ptr() as *const _)))
                    };
                }
                black_box(total) //make sure compiler does not optimise this away
            })
        });
        batch_group.bench_function("asm_strlen_batch", |b| {
            b.iter(|| {
                let mut total = 0;
                for entry in &all_entries {
                    total += unsafe {
                        black_box(asm_strlen(black_box(entry.d_name.as_ptr() as *const _)))
                    };
                }
                black_box(total)
            })
        });

        batch_group.finish();
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(5000)
        .warm_up_time(std::time::Duration::from_millis(500))
        .measurement_time(std::time::Duration::from_secs(3));
    targets = bench_strlen
}

criterion_main!(benches);
