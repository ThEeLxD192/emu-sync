//! Throughput of the SHA-256 that guards every upload, download and restore.
//!
//! Run with `cargo bench -p emusync-data-local --bench hashing`; compare builds with
//! `-- --save-baseline <name>` and `-- --baseline <name>`.

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use emusync_data_local::file_sha256;
use std::{hint::black_box, io::Write};

fn hashing(c: &mut Criterion) {
    let mut group = c.benchmark_group("file_sha256");
    // A memory card and a large folder save; the page cache holds both, so this
    // measures hashing and reads, not the disk.
    for (name, size) in [("128KiB", 128 * 1024), ("64MiB", 64 * 1024 * 1024)] {
        let mut file = tempfile::NamedTempFile::new().expect("temp file");
        let block: Vec<u8> = (0..1024 * 1024).map(|i| (i * 31 % 251) as u8).collect();
        let mut left = size;
        while left > 0 {
            let chunk = left.min(block.len());
            file.write_all(&block[..chunk]).expect("write");
            left -= chunk;
        }
        file.flush().expect("flush");
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_function(name, |b| {
            b.iter(|| black_box(file_sha256(file.path()).expect("hash")));
        });
    }
    group.finish();
}

criterion_group!(benches, hashing);
criterion_main!(benches);
