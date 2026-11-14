//! Cost of reading and recording one synchronization-history entry as the history grows. The history holds
//! every tracked file of the library, and each file of a synchronization looks it up.
//!
//! Run with `cargo bench -p emusync-data-local --bench history`; compare builds with
//! `-- --save-baseline <name>` and `-- --baseline <name>`.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use emusync_data_local::{HistoryCheckpoint, SyncHistoryStore};
use std::hint::black_box;

fn digest(seed: usize) -> String {
    format!("{seed:064x}")
}

fn history(c: &mut Criterion) {
    let mut group = c.benchmark_group("history");
    for entries in [50, 1_000, 5_000] {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = SyncHistoryStore::new(&dir.path().join("config.json")).expect("store");
        let keys: Vec<String> = (0..entries)
            .map(|index| {
                let key = store
                    .key(
                        &dir.path().join(format!("saves/game-{index}.sav")),
                        "folder-id",
                        &format!("game-{index}.sav"),
                    )
                    .expect("key");
                store
                    .record(
                        &key,
                        HistoryCheckpoint {
                            local_sha256: digest(index),
                            remote_id: format!("remote-{index}"),
                            remote_modified_time: "2026-11-13T12:00:00.000Z".into(),
                            remote_sha256: Some(digest(index)),
                            remote_version: Some("7".into()),
                        },
                    )
                    .expect("record");
                key
            })
            .collect();
        group.bench_with_input(BenchmarkId::new("get", entries), &keys, |b, keys| {
            let mut index = 0;
            b.iter(|| {
                index = (index + 1) % keys.len();
                black_box(store.get(&keys[index]).expect("get"))
            });
        });
        // What each uploaded or downloaded file pays: the whole history is rewritten.
        group.bench_with_input(BenchmarkId::new("record", entries), &keys, |b, keys| {
            let mut round = entries;
            b.iter(|| {
                round += 1;
                let checkpoint = HistoryCheckpoint {
                    local_sha256: digest(round),
                    remote_id: "remote-0".into(),
                    remote_modified_time: "2026-11-13T12:00:00.000Z".into(),
                    remote_sha256: Some(digest(round)),
                    remote_version: Some(round.to_string()),
                };
                store.record(&keys[0], checkpoint).expect("record");
            });
        });
    }
    group.finish();
}

criterion_group!(benches, history);
criterion_main!(benches);
