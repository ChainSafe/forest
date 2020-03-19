#[macro_use]
extern crate criterion;

use criterion::{black_box, Criterion, ParameterizedBenchmark, Throughput};
use sector_builder::calculate_checksum;
use tempfile::NamedTempFile;

fn checksum_benchmark(c: &mut Criterion) {
    let params = vec![
        1024,              // 1KiB
        1024 * 1024,       // 1 MiB
        1024 * 1024 * 256, // 256 MiB,
    ];

    c.bench(
        "checksum",
        ParameterizedBenchmark::new(
            "calculate",
            |b, bytes| {
                let mut file = NamedTempFile::new().unwrap();
                file.as_file_mut().set_len(*bytes).unwrap();

                b.iter(|| black_box(calculate_checksum(&file.path())))
            },
            params,
        )
        .sample_size(20)
        .throughput(|bytes| Throughput::Bytes(*bytes)),
    );
}

criterion_group!(benches, checksum_benchmark);
criterion_main!(benches);
