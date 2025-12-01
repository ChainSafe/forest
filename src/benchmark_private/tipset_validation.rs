// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    blocks::Tipset,
    chain::store::ChainStore,
    db::{
        MemoryDB,
        car::{AnyCar, ManyCar},
    },
    genesis::read_genesis_header,
    interpreter::VMTrace,
    networks::{ChainConfig, NetworkChain},
    state_manager::StateManager,
    utils::net::{DownloadFileOption, download_file_with_cache},
};
use criterion::Criterion;
use directories::ProjectDirs;
use std::{
    hint::black_box,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
};
use url::Url;

pub static CACHE_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let project_dir = ProjectDirs::from("com", "ChainSafe", "Forest");
    project_dir
        .map(|d| d.cache_dir().to_path_buf())
        .unwrap_or_else(std::env::temp_dir)
        .join("state_compute_snapshots")
});

async fn get_snapshot(chain: &NetworkChain, epoch: i64) -> anyhow::Result<PathBuf> {
    let url = Url::parse(&format!(
        "https://forest-snapshots.fra1.cdn.digitaloceanspaces.com/state_compute/{chain}_{epoch}.forest.car.zst"
    ))?;
    Ok(
        download_file_with_cache(&url, &CACHE_DIR, DownloadFileOption::NonResumable)
            .await?
            .path,
    )
}

async fn prepare_validation(
    chain: &NetworkChain,
    snapshot: &Path,
) -> anyhow::Result<(Arc<StateManager<ManyCar>>, Tipset)> {
    let snap_car = AnyCar::try_from(snapshot)?;
    let ts = Arc::new(snap_car.heaviest_tipset()?);
    let db = Arc::new(ManyCar::new(MemoryDB::default()).with_read_only(snap_car)?);
    let chain_config = Arc::new(ChainConfig::from_chain(chain));
    let genesis_header =
        read_genesis_header(None, chain_config.genesis_bytes(&db).await?.as_deref(), &db).await?;
    let chain_store = Arc::new(ChainStore::new(
        db.clone(),
        db.clone(),
        db.clone(),
        db.clone(),
        chain_config,
        genesis_header,
    )?);
    let state_manager = Arc::new(StateManager::new(chain_store.clone())?);
    // warmup
    validate(state_manager.clone(), ts.clone()).await;
    Ok((state_manager, ts))
}

async fn validate(state_manager: Arc<StateManager<ManyCar>>, ts: Tipset) {
    state_manager
        .compute_tipset_state(ts, crate::state_manager::NO_CALLBACK, VMTrace::NotTraced)
        .await
        .unwrap();
}

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
                    let snapshot = get_snapshot(&chain, epoch).await?;
                    prepare_validation(&chain, &snapshot).await
                })
                .unwrap();
            b.to_async(&rt)
                .iter(|| validate(black_box(state_manager.clone()), black_box(ts.clone())))
        })
        .bench_function("mainnet@5427431", |b| {
            let chain = NetworkChain::Mainnet;
            let epoch = 5427431;
            let (state_manager, ts) = rt
                .block_on(async {
                    let snapshot = get_snapshot(&chain, epoch).await?;
                    prepare_validation(&chain, &snapshot).await
                })
                .unwrap();
            b.to_async(&rt)
                .iter(|| validate(black_box(state_manager.clone()), black_box(ts.clone())))
        });

    group.finish();
}
