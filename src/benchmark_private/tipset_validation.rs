// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    networks::NetworkChain,
    state_manager::utils::state_compute::{
        get_state_compute_snapshot, prepare_state_compute, state_compute,
    },
};
use criterion::Criterion;
use std::hint::black_box;

pub fn bench_tipset_validation(c: &mut Criterion) {
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
                state_compute(
                    black_box(&state_manager),
                    black_box(ts.clone()),
                    black_box(&ts_next),
                )
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
                state_compute(
                    black_box(&state_manager),
                    black_box(ts.clone()),
                    black_box(&ts_next),
                )
            })
        });

    group.finish();
}
