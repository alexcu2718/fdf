use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

use std::hint::black_box;

#[inline]
pub(crate) const fn repeat_u64(byte: u8) -> u64 {
    u64::from_ne_bytes([byte; size_of::<u64>()])
}

const LO_U64: u64 = repeat_u64(0x01);

const HI_U64: u64 = repeat_u64(0x80);

#[inline]
//modified version to work for this test function(copy pasted really)
pub const unsafe fn dirent_const_time_strlen(dirent: *const LibcDirent64) -> usize {
    // Offset from the start of the struct to the beginning of d_name.
    const DIRENT_HEADER_START: usize = core::mem::offset_of!(LibcDirent64, d_name);
    // Access the last field and then round up to find the minimum struct size
    const MINIMUM_DIRENT_SIZE: usize = DIRENT_HEADER_START.next_multiple_of(8);

    use core::num::NonZeroU64;

    /*  Accessing `d_reclen` is safe because the struct is kernel-provided.
    / SAFETY: `dirent` is valid by precondition */
    let reclen = unsafe { (*dirent).d_reclen } as usize;

    /*
      Read the last 8 bytes of the struct as a u64.
    This works because dirents are always 8-byte aligned. */
    // SAFETY: We're indexing in bounds within the pointer (it is guaranteed aligned by the kernel)
    let last_word: u64 = unsafe { *(dirent.cast::<u8>()).add(reclen - 8).cast::<u64>() };
    /* Note, I don't index as a u64 with eg (reclen-8)/8 or (reclen-8)>>3 because that adds a division which is a costly operation, relatively speaking
    let last_word: u64 = unsafe { *(dirent.cast::<u64>()).add((reclen - 8)/8 (or >>3))}; //this will also work but it's less performant (MINUTELY)
    */

    #[cfg(target_endian = "little")]
    const MASK: u64 = 0x00FF_FFFFu64;
    #[cfg(target_endian = "big")]
    const MASK: u64 = 0xFFFF_FF00_0000_0000u64; // byte order is shifted unintuitively on big endian!

    /* When the record length is 24/`MINIMUM_DIRENT_SIZE`, the kernel may insert nulls before d_name.
    Which will exist on index's 17/18 (or opposite, for big endian...sigh.,)
    Mask them out to avoid false detection of a terminator.
    Multiplying by 0 or 1 applies the mask conditionally without branching. */
    let mask: u64 = MASK * ((reclen == MINIMUM_DIRENT_SIZE) as u64);
    /*
     Apply the mask to ignore non-name bytes while preserving name bytes.
     Result:
     - Name bytes remain unchanged
     - Non-name bytes become 0xFF (guaranteed non-zero)
     - Any null terminator in the name remains detectable
    */
    let candidate_pos: u64 = last_word | mask;

    /*
     Locate the first null byte in constant time using SWAR.
     Subtract  the position of the index of the 0 then add 1 to compute its position relative to the start of d_name.

     SAFETY: The u64 can never be all 0's post-SWAR, therefore we can make a niche optimisation
     https://doc.rust-lang.org/std/num/struct.NonZero.html#tymethod.trailing_zeros
     https://doc.rust-lang.org/std/num/struct.NonZero.html#tymethod.leading_zeros
    (using ctlz_nonzero instruction which is superior to ctlz but can't handle all 0 numbers)
    */
    let zero_bit = unsafe {
        NonZeroU64::new_unchecked(candidate_pos.wrapping_sub(LO_U64) & !candidate_pos & HI_U64)
    };
    #[cfg(target_endian = "little")]
    let byte_pos = 8 - (zero_bit.trailing_zeros() >> 3) as usize;
    #[cfg(not(target_endian = "little"))]
    let byte_pos = 8 - (zero_bit.leading_zeros() >> 3) as usize;

    /*  Final length:
    total record length - header size - null byte position
    */
    reclen - DIRENT_HEADER_START - byte_pos
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
