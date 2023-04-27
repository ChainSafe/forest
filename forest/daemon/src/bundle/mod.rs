// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_cli_shared::cli::Config;
use forest_genesis::forest_load_car;
use forest_networks::Height;
use forest_shim::clock::ChainEpoch;
use forest_utils::net::FetchProgress;
use fvm_ipld_blockstore::Blockstore;
use log::info;
use tokio::{
    fs::File,
    io::{BufReader, BufWriter},
};
use tokio_util::compat::{FuturesAsyncReadCompatExt, TokioAsyncReadCompatExt};

pub async fn load_bundles<DB>(epoch: ChainEpoch, config: &Config, db: DB) -> anyhow::Result<()>
where
    DB: Blockstore + Send + Sync + Clone + 'static,
{
    // collect bundles to load into the database.
    let mut bundles = Vec::new();
    if epoch < config.chain.epoch(Height::Hygge) {
        bundles.push(get_actors_bundle(config, Height::Hygge).await?);
    }
    if epoch < config.chain.epoch(Height::Lightning) {
        bundles.push(get_actors_bundle(config, Height::Lightning).await?);
    }
    // Nothing to do regarding Thunder since it's more like a "ghost" upgrade.

    for bundle in bundles {
        let result = forest_load_car(db.clone(), bundle.compat()).await?;
        assert_eq!(
            result.len(),
            1,
            "expected one root when loading actors bundle"
        );
        info!("Loaded actors bundle with CID: {}", result[0]);
    }
    Ok(())
}

/// Downloads the actors bundle (if not already downloaded) and returns a reader
/// to it.
// TODO Get it from IPFS instead of GitHub.
pub async fn get_actors_bundle(config: &Config, height: Height) -> anyhow::Result<BufReader<File>> {
    let bundle_info = config.chain.height_infos[height as usize]
        .bundle
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no bundle for epoch {}", config.chain.epoch(height)))?;

    // This is the path where the actors bundle will be stored.
    let bundle_path_dir = config
        .client
        .data_dir
        .join("bundles")
        .join(&config.chain.name);

    tokio::fs::create_dir_all(&bundle_path_dir).await?;
    let bundle_path = bundle_path_dir.join(format!("bundle_{height}.car"));

    // If the bundle already exists, return a reader to it.
    if bundle_path.exists() {
        let file = tokio::fs::File::open(bundle_path).await?;
        return Ok(BufReader::new(file));
    }

    // Otherwise, download it.
    info!("Downloading actors bundle...");
    let reader = FetchProgress::fetch_from_url(&bundle_info.url).await?.inner;

    let file = File::create(&bundle_path).await?;
    let mut writer = BufWriter::new(file);
    tokio::io::copy(&mut reader.compat(), &mut writer).await?;

    let file = tokio::fs::File::open(bundle_path).await?;
    Ok(BufReader::new(file))
}
