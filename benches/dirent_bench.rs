use core::hint::black_box;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use fdf::util::{memrchr, memrchr_old};

fn bench_memrchr(c: &mut Criterion) {
    let mut group = c.benchmark_group("memrchr");

    let sizes = [16usize, 64, 256, 1024, 8 * 1024, 64 * 1024];

    for size in sizes {
        let mut data = vec![0u8; size];
        data[size / 2] = 1;
        data[size - 1] = 1;

        let needle = 1u8;

        group.bench_with_input(BenchmarkId::new("old", size), &data, |b, data| {
            b.iter(|| black_box(memrchr_old(black_box(needle), black_box(data))))
        });

        group.bench_with_input(BenchmarkId::new("new", size), &data, |b, data| {
            b.iter(|| black_box(memrchr(black_box(needle), black_box(data))))
        });
    }

    group.finish();
}

criterion_group!(benches, bench_memrchr);
criterion_main!(benches);
