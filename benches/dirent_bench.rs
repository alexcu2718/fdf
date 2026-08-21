#![allow(clippy::all)]
#![allow(clippy::pedantic)]
#![allow(clippy::restriction)]
#![allow(clippy::nursery)]

use core::{mem::offset_of, num::NonZeroU64};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use std::hint::black_box;

const MAX_DIRENT_SIZE: usize = 2000; // 'officially' its 256
#[allow(non_camel_case_types)]
type dirent64 = LibcDirent64;

/// Modified version to work for this test function(copy pasted really)
#[inline]
pub const unsafe fn dirent_const_time_strlen(drnt: *const dirent64) -> usize {
    // On these systems where we need a bit of 'black magic' (no d_namlen field)

    // Offset from the start of the struct to the beginning of d_name.
    const DIRENT_HEADER_START: usize = offset_of!(dirent64, d_name);
    // Access the last field and then round up to find the minimum struct size
    const MIN_DIRENT_SIZE: usize = DIRENT_HEADER_START.next_multiple_of(8);
    // Compile time assert to immediately cancel the build if invalidated
    const { assert!(MIN_DIRENT_SIZE == 24, "dirent min size must be 24!") };
    const LO_U64: u64 = !0 / 0xFF;
    const HI_U64: u64 = LO_U64 * 0x80;

    /*  SAFETY: `dirent` is valid by precondition */
    let reclen = unsafe { (*drnt).d_reclen } as usize;

    let mut last_word: u64 = unsafe { drnt.byte_add(reclen - 8).cast::<u64>().read() };

    #[cfg(target_endian = "little")]
    let mask = (reclen as u64).wrapping_sub(25) >> 40;

    #[cfg(target_endian = "big")]
    let mask = (reclen as u64).wrapping_sub(25) & 0xFFFF_FF00_0000_0000;
    last_word |= mask;

    //SAFETY: The u64 can never be all 0's post-mask because the last word ALWAYS contains at least one NUL, which become 0x80
    #[cfg(target_endian = "little")]
    let masked_word =
        unsafe { NonZeroU64::new_unchecked(last_word.wrapping_sub(LO_U64) & !last_word & HI_U64) };

    #[cfg(target_endian = "big")]
    //SAFETY: as in LE version.
    let masked_word = unsafe {
        NonZeroU64::new_unchecked(
            (!last_word & !HI_U64).wrapping_add(LO_U64) & (!last_word & HI_U64),
        )
    };

    // Find the position of the null terminator
    #[cfg(target_endian = "little")]
    let byte_pos = (masked_word.trailing_zeros() >> 3) as usize;
    #[cfg(target_endian = "big")]
    let byte_pos = (masked_word.leading_zeros() >> 3) as usize;

    reclen - DIRENT_HEADER_START + byte_pos - 8
}

#[repr(C)]
pub struct LibcDirent64 {
    // Fake a structure similar to libc::dirent64
    pub d_ino: u64,
    pub d_off: u64,
    pub d_reclen: u16,
    pub d_type: u8,
    pub d_name: [u8; MAX_DIRENT_SIZE],
}

const fn calculate_min_reclen(name_len: usize) -> u16 {
    const HEADER_SIZE: usize = offset_of!(dirent64, d_name);
    let total_size = HEADER_SIZE + name_len + 1;
    total_size.next_multiple_of(8) as _
    //reclen follows specification: must be multiple of 8 and at least 24 bytes but we calculate the reclen based on the name length
    //this works because it's given the same representation in memory so repr C will ensure the layout is compatible
}

fn make_dirent(name: &str) -> dirent64 {
    let bytes = name.as_bytes();
    assert!(
        bytes.len() < MAX_DIRENT_SIZE,
        "Name too long for dirent structure"
    );

    let min_reclen = calculate_min_reclen(bytes.len());
    let mut entry = LibcDirent64 {
        d_ino: 0,
        d_off: 0,
        d_reclen: min_reclen,
        d_type: 0,
        d_name: [0; MAX_DIRENT_SIZE],
    };

    let (name_bytes, tail) = entry.d_name.split_at_mut(bytes.len());
    name_bytes.copy_from_slice(bytes);

    if let Some(null_byte) = tail.first_mut() {
        *null_byte = 0;
    }

    entry
}

fn bench_strlen(c: &mut Criterion) {
    let mut length_groups = vec![
        ("length=1", "a".to_owned()),
        ("length=16", "b".repeat(16)),
        ("length=32", "c".repeat(32)),
        ("length=64", "y".repeat(64)),
        ("length=128 ", "a".repeat(128)),
        ("length=200", "b".repeat(200)),
        ("length=255", "b".repeat(255)),
    ];

    if MAX_DIRENT_SIZE > 300 {
        length_groups.push(("length=320", "c".repeat(320)))
    }

    if MAX_DIRENT_SIZE > 520 {
        length_groups.push(("length=512", "o".repeat(512)))
    }

    if MAX_DIRENT_SIZE > 1040 {
        length_groups.push(("length=1024", "o".repeat(1024)))
    }

    let all_entries: Vec<_> = length_groups
        .iter()
        .map(|entry| make_dirent(&entry.1))
        .collect();

    //  make separate benchmark groups one at a time
    {
        let mut group = c.benchmark_group("strlen_by_length");

        for (size_name, name) in length_groups {
            let entry = make_dirent(&name);
            let byte_len = name.len();

            group.throughput(Throughput::Bytes(byte_len as u64));

            group.bench_with_input(
                BenchmarkId::new("const_time_swar", size_name),
                &entry,
                |b, e| {
                    // SAFETY: `e` points to a live `LibcDirent64` created in this benchmark.
                    b.iter(|| unsafe {
                        black_box(dirent_const_time_strlen(black_box(core::ptr::from_ref(e))))
                    })
                },
            );

            group.bench_with_input(
                BenchmarkId::new("libc_strlen", size_name),
                &entry,
                |b, e| {
                    b.iter(|| unsafe {
                        black_box(libc::strlen(black_box((&raw const (*e).d_name).cast())))
                    })
                },
            );
        }
        group.finish();
    };

    //  create the batch comparison group
    {
        let mut batch_group = c.benchmark_group("strlen_batch_comparison");
        batch_group.throughput(Throughput::Elements(all_entries.len() as u64));

        batch_group.bench_function("const_time_swar_batch", |b| {
            b.iter(|| {
                let mut total = 0;
                for entry in &all_entries {
                    total += unsafe {
                        black_box(dirent_const_time_strlen(black_box(core::ptr::from_ref(
                            entry,
                        ))))
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
                        black_box(libc::strlen(black_box((&raw const (*entry).d_name).cast())))
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
