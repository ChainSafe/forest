// Copyright 2019-2025 ChainSafe Systems
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
        .bench_function("calibnet@3111900", |b| {
            let chain = NetworkChain::Calibnet;
            let epoch = 3111900;
            let (state_manager, ts) = rt
                .block_on(async {
                    let snapshot = get_state_compute_snapshot(&chain, epoch).await?;
                    prepare_state_compute(&chain, &snapshot, true).await
                })
                .unwrap();
            b.to_async(&rt)
                .iter(|| state_compute(black_box(state_manager.clone()), black_box(ts.clone())))
        })
        .bench_function("mainnet@5427431", |b| {
            let chain = NetworkChain::Mainnet;
            let epoch = 5427431;
            let (state_manager, ts) = rt
                .block_on(async {
                    let snapshot = get_state_compute_snapshot(&chain, epoch).await?;
                    prepare_state_compute(&chain, &snapshot, true).await
                })
                .unwrap();
            b.to_async(&rt)
                .iter(|| state_compute(black_box(state_manager.clone()), black_box(ts.clone())))
        });

    group.finish();
}
