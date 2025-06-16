

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use std::hint::black_box;
use fdf::dirent_const_time_strlen;

//use std::hint::assert_unchecked;
use libc::{dirent64, c_char};

#[repr(C, align(8))]
pub struct LibcDirent64 { //fake a strujcture similar to libc::dirent64
    pub d_ino: u64,
    pub d_off: u64,
    pub d_reclen: u16,
    pub d_type: u8,
    pub d_name: [u8; 256],
}

// Calculate minimum required reclen (rounds up to nearest multiple of 8)
const fn calculate_min_reclen(name_len: usize) -> u16 {
    const HEADER_SIZE: usize = std::mem::offset_of!(LibcDirent64, d_name);
    let total_size = HEADER_SIZE + name_len + 1; // +1 for null terminator
    ((total_size + 7) & !7) as u16 // Round up to nearest multiple of 8
}

 fn make_dirent(name: &str) -> dirent64 {
    let bytes = name.as_bytes();
    assert!(bytes.len() < 256, "Name too long for dirent structure");
    
    let min_reclen = calculate_min_reclen(bytes.len());
    assert!(min_reclen >= 24, "Reclen must be at least 24 bytes");
    
    let mut entry = LibcDirent64 {
        d_ino: 0,
        d_off: 0,
        d_reclen: min_reclen,
        d_type: 0,
        d_name: [0; 256],
    };

    entry.d_name[..bytes.len()].copy_from_slice(bytes);
    entry.d_name[bytes.len()] = 0; // Null-terminate
    
    // Validate reclen meets requirements
   assert!(entry.d_reclen as usize >= std::mem::size_of::<u64>() * 3, 
           "Reclen must be at least 24 bytes (3*u64)");
    assert_eq!(entry.d_reclen % 8, 0, "Reclen must be multiple of 8");
    
    unsafe { std::mem::transmute(entry) } //construct it articially...oh boy we got rules to follow now.
}



fn bench_strlen(c: &mut Criterion) {
    // Create more diverse test cases with random elements to prevent constant folding
    let mut test_entries = Vec::new();
    let base_names = [
        "",
        "a",
        "file1", 
        "document.txt",
        "file_with_medium_length_name",
        "very_long_filename_that_simulates_real_world_usage_patterns",
        &"x".repeat(50),
        &"abcdefghij".repeat(20), // Creates ~200char name  
        &"z".repeat(254), // Long but under 255 limit
    ];
    
    // Create test entries with some variation 
    for (i, base_name) in base_names.iter().enumerate() {
        let varied_name = if base_name.is_empty() {
            String::new()
        } else {
            format!("{}{}", base_name, i % 10) // Add variation
        };
        test_entries.push((format!("case_{}", i), make_dirent(&varied_name)));
    }
    
    let mut group = c.benchmark_group("strlen_comparison");
    group.throughput(Throughput::Elements(test_entries.len() as u64));
    
    // Benchmark processing multiple entries to get more realistic results for my tool
    group.bench_function("dirent_const_time_batch", |b| {
        b.iter(|| {
            let mut total = 0;
            for (_, entry) in &test_entries {//blackbox all operations to prevent optimizations(compiler will noop it away)
                total += unsafe { black_box(dirent_const_time_strlen(black_box(entry))) };
            }
            black_box(total)
        })
    });
    
    group.bench_function("libc_strlen_batch", |b| {
        b.iter(|| {
            let mut total = 0;
            for (_, entry) in &test_entries {
                total += unsafe { 
                    //same as above
                    black_box(libc::strlen(black_box(entry.d_name.as_ptr() as *const c_char))) 
                };
            }
            black_box(total)
        })
    });
    
    // Also benchmark individual cases for detailed analysis
    for (name, entry) in &test_entries {
        group.bench_with_input(
            BenchmarkId::new("dirent_const_time_single", name),
            entry,
            |b, e| b.iter(|| unsafe {
                // Use the result to prevent dead code elimination
                let result = dirent_const_time_strlen(black_box(e));
                black_box(result)
            })
        );
        
        group.bench_with_input(
            BenchmarkId::new("libc_strlen_single", name),
            entry,
            |b, e| b.iter(|| unsafe {
                let result = libc::strlen(black_box(e.d_name.as_ptr() as *const c_char));
                black_box(result)
            })
        );
    }
    
    group.finish();
}


criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(1000)  
        .warm_up_time(std::time::Duration::from_millis(500))
        .measurement_time(std::time::Duration::from_secs(3));
    targets = bench_strlen
}
criterion_main!(benches);