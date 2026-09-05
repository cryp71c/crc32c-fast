use std::hint::black_box;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

use crc32c_fast::crc32c;

fn bench_crc32c(c: &mut Criterion) {
    let mut group = c.benchmark_group("crc32c_api");
    group.measurement_time(Duration::from_secs(15));

    let sizes = [64usize, 4 * 1024, 1024 * 1024, 16 * 1024 * 1024];

    for size in sizes {
        group.throughput(Throughput::Bytes(size as u64));

        let data = vec![0xA5u8; size];

        let benchmark_id = BenchmarkId::new("crc32c_fast", size);

        group.bench_with_input(benchmark_id, &data, |b, data| {
            b.iter(|| black_box(crc32c(black_box(data))));
        });

        let benchmark_id = BenchmarkId::new("reference", size);
        group.bench_with_input(benchmark_id, &data, |b, data| {
            b.iter(|| black_box(crc32c::crc32c(black_box(data))));
        });
    }

    group.finish();
}

criterion_group!(benches, bench_crc32c,);
criterion_main!(benches);
