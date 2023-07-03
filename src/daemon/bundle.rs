// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::cli_shared::cli::Config;
use crate::genesis::forest_load_car;
use crate::networks::Height;
use crate::shim::clock::ChainEpoch;
use fvm_ipld_blockstore::Blockstore;
use log::info;
use tokio::{
    fs::File,
    io::{BufReader, BufWriter},
};
use tokio_util::compat::TokioAsyncReadCompatExt;

pub async fn load_bundles<DB>(epoch: ChainEpoch, config: &Config, db: DB) -> anyhow::Result<()>
where
    DB: Blockstore + Send + Sync + Clone + 'static,
{
    // collect bundles to load into the database.
    let mut bundles = Vec::new();
    for info in &config.chain.height_infos {
        if epoch < config.chain.epoch(info.height) {
            if let Some(bundle) = &info.bundle {
                bundles.push((
                    bundle.manifest,
                    get_actors_bundle(config, info.height).await?,
                ));
            }
        }
    }

    for (manifest_cid, reader) in bundles {
        let (result, _) = forest_load_car(db.clone(), reader.compat()).await?;
        assert_eq!(
            result.len(),
            1,
            "expected one root when loading actors bundle"
        );
        info!("Loaded actors bundle with CID: {}", result[0]);
        anyhow::ensure!(
            manifest_cid == result[0],
            "manifest cid in config '{manifest_cid}' does not match manifest cid from bundle '{}'",
            result[0]
        );
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
        .join(config.chain.network.to_string());

    tokio::fs::create_dir_all(&bundle_path_dir).await?;
    let bundle_path = bundle_path_dir.join(format!("bundle_{height}.car"));

    // If the bundle already exists, return a reader to it.
    if bundle_path.exists() {
        let file = tokio::fs::File::open(bundle_path).await?;
        return Ok(BufReader::new(file));
    }

    // Otherwise, download it.
    info!("Downloading actors bundle...");
    let mut reader = crate::utils::net::reader(bundle_info.url.as_str()).await?;

    let file = File::create(&bundle_path).await?;
    let mut writer = BufWriter::new(file);
    tokio::io::copy(&mut reader, &mut writer).await?;

    let file = tokio::fs::File::open(bundle_path).await?;
    Ok(BufReader::new(file))
}
