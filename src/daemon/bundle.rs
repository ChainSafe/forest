// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    networks::{ActorBundleInfo, NetworkChain, ACTOR_BUNDLES},
    utils::{
        db::{
            car_stream::{CarBlock, CarStream},
            car_util::load_car,
        },
        net::http_get,
    },
};
use ahash::HashSet;
use anyhow::ensure;
use futures::{stream::FuturesUnordered, TryStreamExt};
use fvm_ipld_blockstore::Blockstore;
use std::mem::discriminant;
use std::{io::Cursor, path::Path};
use tracing::{info, warn};

/// Tries to load the missing actor bundles to the blockstore. If the bundle is
/// not present, it will be downloaded.
pub async fn load_actor_bundles(
    db: &impl Blockstore,
    network: &NetworkChain,
) -> anyhow::Result<()> {
    if let Some(bundle_path) = match std::env::var("FOREST_ACTOR_BUNDLE_PATH") {
        Ok(path) if !path.is_empty() => Some(path),
        _ => None,
    } {
        info!(%bundle_path, "loading actor bundle set by FOREST_ACTOR_BUNDLE_PATH environment variable");
        load_actor_bundles_from_path(db, network, &bundle_path).await
    } else {
        load_actor_bundles_from_server(db, network).await
    }
}

pub async fn load_actor_bundles_from_path(
    db: &impl Blockstore,
    network: &NetworkChain,
    bundle_path: impl AsRef<Path>,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        bundle_path.as_ref().is_file(),
        "Bundle file not found at {}",
        bundle_path.as_ref().display()
    );
    let mut car_stream = CarStream::new(tokio::io::BufReader::new(
        tokio::fs::File::open(bundle_path.as_ref()).await?,
    ))
    .await?;

    // Validate the bundle
    let roots = HashSet::from_iter(car_stream.header.roots.iter());
    for ActorBundleInfo {
        manifest, network, ..
    } in ACTOR_BUNDLES.iter().filter(|bundle| {
        // Comparing only the discriminant is enough. All devnets share the same
        // actor bundle.
        discriminant(network) == discriminant(&bundle.network)
    }) {
        anyhow::ensure!(
                roots.contains(manifest),
                "actor {manifest} for {network} is missing from {}, try regenerating the bundle with `forest-tool state-migration actor-bundle`", bundle_path.as_ref().display());
    }

    // Load into DB
    while let Some(CarBlock { cid, data }) = car_stream.try_next().await? {
        db.put_keyed(&cid, &data)?;
    }

    Ok(())
}

pub async fn load_actor_bundles_from_server(
    db: &impl Blockstore,
    network: &NetworkChain,
) -> anyhow::Result<()> {
    FuturesUnordered::from_iter(
        ACTOR_BUNDLES
            .iter()
            .filter(|bundle| {
                !db.has(&bundle.manifest).unwrap_or(false) &&
                // Comparing only the discriminant is enough. All devnets share the same
                // actor bundle.
                discriminant(network) == discriminant(&bundle.network)
            })
            .map(
                |ActorBundleInfo {
                     manifest: root,
                     url,
                     alt_url,
                     network: _,
                 }| async move {
                    let response = if let Ok(response) = http_get(url).await {
                        response
                    } else {
                        warn!("failed to download bundle from primary URL, trying alternative URL");
                        http_get(alt_url).await?
                    };
                    let bytes = response.bytes().await?;
                    let header = load_car(db, Cursor::new(bytes)).await?;
                    ensure!(header.roots.len() == 1);
                    ensure!(header.roots.first() == root);
                    Ok(())
                },
            ),
    )
    .try_collect::<Vec<_>>()
    .await?;

    Ok(())
}
