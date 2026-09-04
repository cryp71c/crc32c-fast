#[cfg(target_arch = "x86_64")]
#[path = "./experimental/x86_64_parallel.rs"]
mod x86_64_parallel;

use crc32c_fast::__bench::portable;

#[cfg(target_arch = "x86_64")]
use crc32c_fast::__bench::x86_64;

// Matches the gate in the crate root: the AArch64 backend does not exist on
// big-endian targets, so neither does this benchmark arm.
#[cfg(all(target_arch = "aarch64", target_endian = "little"))]
use crc32c_fast::__bench::aarch64;

use std::hint::black_box;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};

#[cfg(target_arch = "x86_64")]
const STREAM_SIZES: &[usize] = &[
    768,         // 768 B
    768 * 8,     // 6 KiB
    768 * 64,    // 48 KiB
    768 * 1365,  // 1,048,320 B  ≈ 1 MiB
    768 * 21845, // 16,776,960 B ≈ 16 MiB
];

fn bench_crc32c(c: &mut Criterion) {
    let sizes = [64usize, 4 * 1024, 1024 * 1024, 16 * 1024 * 1024];

    let mut group = c.benchmark_group("crc32c");
    group.measurement_time(Duration::from_secs(15));

    for size in sizes {
        group.throughput(Throughput::Bytes(size as u64));

        // create a vector of x size with 0xA5 unsigned 8 bit
        let data = vec![0xA5u8; size];

        let benchmark_id = BenchmarkId::new("portable", size);

        group.bench_with_input(benchmark_id, &data, |b, data| {
            b.iter(|| black_box(portable::crc32c_u32(black_box(data))));
        });

        let benchmark_id = BenchmarkId::new("reference", size);
        group.bench_with_input(benchmark_id, &data, |b, data| {
            b.iter(|| black_box(crc32c::crc32c(black_box(data))));
        });

        #[cfg(all(target_arch = "aarch64", target_endian = "little"))]
        {
            let benchmark_id = BenchmarkId::new("aarch64", size);

            group.bench_with_input(benchmark_id, &data, |b, data| {
                b.iter(|| black_box(unsafe { aarch64::crc32c_u32(black_box(data)) }));
            });
        }

        #[cfg(target_arch = "x86_64")]
        {
            let benchmark_id = BenchmarkId::new("x86_64", size);

            group.bench_with_input(benchmark_id, &data, |b, data| {
                b.iter(|| black_box(unsafe { x86_64::crc32c_u32(black_box(data)) }));
            });
        }
    }

    group.finish();
}

#[cfg(target_arch = "x86_64")]
fn bench_crc32c_parallel_768(c: &mut Criterion) {
    let size = 768usize;
    let data = vec![0xA5u8; size];

    let mut group = c.benchmark_group("crc32c_parallel_768");
    group.throughput(Throughput::Bytes(size as u64));

    let benchmark_id = BenchmarkId::new("x86_64", size);
    group.bench_with_input(benchmark_id, &data, |b, data| {
        b.iter(|| black_box(unsafe { x86_64::crc32c_u32(black_box(data)) }));
    });

    let benchmark_id = BenchmarkId::new("x86_64_parallel", size);
    group.bench_with_input(benchmark_id, &data, |b, data| {
        b.iter(|| black_box(unsafe { x86_64_parallel::crc32c_u32(black_box(data)) }));
    });

    let benchmark_id = BenchmarkId::new("reference", size);
    group.bench_with_input(benchmark_id, &data, |b, data| {
        b.iter(|| black_box(crc32c::crc32c(black_box(data))));
    });

    group.finish();
}

#[cfg(target_arch = "x86_64")]
fn bench_crc32c_parallel_streaming(c: &mut Criterion) {
    let mut group = c.benchmark_group("crc32c_parallel_streaming");
    group.measurement_time(Duration::from_secs(15));

    for &size in STREAM_SIZES {
        group.throughput(Throughput::Bytes(size as u64));

        let data = vec![0xA5u8; size];

        let benchmark_id = BenchmarkId::new("x86_64", size);
        group.bench_with_input(benchmark_id, &data, |b, data| {
            b.iter(|| black_box(unsafe { x86_64::crc32c_u32(black_box(data)) }));
        });

        let benchmark_id = BenchmarkId::new("x86_64_parallel", size);
        group.bench_with_input(benchmark_id, &data, |b, data| {
            b.iter(|| black_box(unsafe { x86_64_parallel::crc32c_u32(black_box(data)) }));
        });
    }

    group.finish();
}

#[cfg(target_arch = "x86_64")]
criterion_group!(
    benches,
    bench_crc32c,
    bench_crc32c_parallel_768,
    bench_crc32c_parallel_streaming
);

#[cfg(not(target_arch = "x86_64"))]
criterion_group!(benches, bench_crc32c);

criterion_main!(benches);
