// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Benchmark harness (feature-gated, not node runtime); unwrapping setup failures is acceptable here.
#![allow(clippy::unwrap_used)]

use crate::{
    blocks::Tipset,
    interpreter::VMTrace,
    networks::NetworkChain,
    state_manager::{
        NO_CALLBACK,
        utils::state_compute::{get_state_compute_snapshot, prepare_state_compute, state_compute},
    },
};
use anyhow::Context as _;
use criterion::Criterion;
use std::hint::black_box;
use std::path::Path;
use std::time::Instant;

// Re-validate a range of tipsets from a local snapshot, timing each, so the cost of tipset
// validation can be compared across FVM/wasmtime changes on real chain data.
//
// Knobs:
//   FOREST_BENCH_SNAPSHOT  path to a full `.forest.car.zst` snapshot (required to enable this mode)
//   FOREST_BENCH_CHAIN     network name, e.g. `mainnet` or `calibnet` (default: mainnet)
//   FOREST_BENCH_EPOCHS    how many tipsets to walk back from the snapshot head (default: 1).
//                          Must fit inside the snapshot's retained-state window, otherwise the run
//                          fails rather than reporting a short result.
fn validate_range(chain: &NetworkChain, snapshot: &Path, epochs: u64) -> anyhow::Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    anyhow::ensure!(epochs > 0, "FOREST_BENCH_EPOCHS must be greater than 0");
    rt.block_on(async {
        let (sm, mut ts, mut child) = prepare_state_compute(chain, snapshot)
            .await
            .with_context(|| format!("failed to open snapshot {}", snapshot.display()))?;
        let head = ts.epoch();
        // Recomputed state accumulates in the in-memory write layer over the snapshot, so a very
        // large range against an archival snapshot (which retains every state) can grow unbounded.
        let mut durations_ms: Vec<f64> = Vec::with_capacity(epochs.min(10_000) as usize);
        let wall = Instant::now();
        for i in 0..epochs {
            let t0 = Instant::now();
            let executed = match sm
                .compute_tipset_state(ts.clone(), NO_CALLBACK, VMTrace::NotTraced)
                .await
            {
                Ok(e) => e,
                Err(e) => {
                    // Never report a short run as a success: a partial walk is either bad input
                    // (asking for more tipsets than the snapshot retains) or a real regression, and
                    // both deserve a non-zero exit.
                    return Err(anyhow::Error::new(e)).with_context(|| {
                        format!(
                            "failed to compute tipset state at epoch {} after validating {} of \
                             {epochs} tipsets; if the snapshot's retained-state window is shallower \
                             than the requested range, lower FOREST_BENCH_EPOCHS",
                            ts.epoch(),
                            durations_ms.len()
                        )
                    });
                }
            };
            durations_ms.push(t0.elapsed().as_secs_f64() * 1e3);
            anyhow::ensure!(
                executed.state_root == *child.parent_state(),
                "state root mismatch at epoch {}: computed {}, expected {}",
                ts.epoch(),
                executed.state_root,
                child.parent_state()
            );
            anyhow::ensure!(
                executed.receipt_root == *child.parent_message_receipts(),
                "receipt root mismatch at epoch {}: computed {}, expected {}",
                ts.epoch(),
                executed.receipt_root,
                child.parent_message_receipts()
            );
            if i + 1 == epochs {
                break;
            }
            let parent = Tipset::load_required(sm.db(), ts.parents())
                .with_context(|| format!("failed to load parent of epoch {}", ts.epoch()))?;
            child = ts;
            ts = parent;
        }

        let total = wall.elapsed().as_secs_f64();
        let n = durations_ms.len();
        anyhow::ensure!(n > 0, "no tipsets were validated");
        let sum: f64 = durations_ms.iter().sum();
        let cold = *durations_ms
            .first()
            .context("no timing samples were collected")?;
        durations_ms.sort_unstable_by(f64::total_cmp);
        let sample = |i: usize| -> anyhow::Result<f64> {
            durations_ms
                .get(i)
                .copied()
                .with_context(|| format!("timing sample {i} out of range (n = {n})"))
        };
        let median = if n.is_multiple_of(2) {
            (sample(n / 2 - 1)? + sample(n / 2)?) / 2.0
        } else {
            sample(n / 2)?
        };
        let p95 = sample((n as f64 * 0.95) as usize)?;

        println!("\n==== tipset validation timing ({chain}) ====");
        println!("tipsets validated (from epoch {head}) : {n}");
        println!("total wall time            : {total:.2} s");
        println!("mean per tipset            : {:.2} ms", sum / n as f64);
        println!("median per tipset          : {median:.2} ms");
        println!("p95 per tipset             : {p95:.2} ms");
        println!("first tipset (cold compile): {cold:.1} ms");
        anyhow::Ok(())
    })
}

/// Benchmarks tipset validation.
///
/// By default this runs the criterion benchmarks over purpose-built snapshots.
/// Setting `FOREST_BENCH_SNAPSHOT` instead validates and times a range of tipsets from
/// a local snapshot; see [`validate_range`] for the accompanying environment variables.
pub fn bench_tipset_validation(c: &mut Criterion) {
    if let Ok(path) = std::env::var("FOREST_BENCH_SNAPSHOT") {
        let chain = match std::env::var("FOREST_BENCH_CHAIN").as_deref() {
            Ok("mainnet") | Err(_) => NetworkChain::Mainnet,
            Ok("calibnet") => NetworkChain::Calibnet,
            Ok(other) => {
                panic!("FOREST_BENCH_CHAIN must be `mainnet` or `calibnet`, got {other:?}")
            }
        };
        let epochs: u64 = std::env::var("FOREST_BENCH_EPOCHS")
            .ok()
            .map(|s| s.parse().expect("invalid FOREST_BENCH_EPOCHS"))
            .unwrap_or(1);
        validate_range(&chain, Path::new(&path), epochs).expect("tipset validation failed");
        return;
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    let mut group = c.benchmark_group("tipset_validation");

    group
        .bench_function("calibnet@3408952", |b| {
            let chain = NetworkChain::Calibnet;
            let epoch = 3408952;
            let (state_manager, ts, ts_next) = rt
                .block_on(async {
                    let snapshot = get_state_compute_snapshot(&chain, epoch).await?;
                    prepare_state_compute(&chain, &snapshot).await
                })
                .unwrap();
            b.to_async(&rt).iter(|| {
                black_box(state_compute(
                    black_box(&state_manager),
                    black_box(ts.clone()),
                    black_box(&ts_next),
                ))
            })
        })
        .bench_function("mainnet@5709604", |b| {
            let chain = NetworkChain::Mainnet;
            let epoch = 5709604;
            let (state_manager, ts, ts_next) = rt
                .block_on(async {
                    let snapshot = get_state_compute_snapshot(&chain, epoch).await?;
                    prepare_state_compute(&chain, &snapshot).await
                })
                .unwrap();
            b.to_async(&rt).iter(|| {
                black_box(state_compute(
                    black_box(&state_manager),
                    black_box(ts.clone()),
                    black_box(&ts_next),
                ))
            })
        });

    group.finish();
}
