// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    networks::{ActorBundleInfo, NetworkChain, ACTOR_BUNDLES},
    utils::{db::car_util::load_car, net::http_get},
};
use anyhow::ensure;
use futures::{stream::FuturesUnordered, TryStreamExt};
use fvm_ipld_blockstore::Blockstore;
use std::io::Cursor;
use std::mem::discriminant;
use tracing::warn;

/// Tries to load the missing actor bundles to the blockstore. If the bundle is
/// not present, it will be downloaded.
pub async fn load_actor_bundles(
    db: &impl Blockstore,
    network: &NetworkChain,
) -> anyhow::Result<()> {
    FuturesUnordered::from_iter(
        ACTOR_BUNDLES
            .iter()
            .filter(|bundle| {
                !db.has(&bundle.manifest).unwrap_or(false)
                    && discriminant(network) == discriminant(&bundle.network)
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
                    ensure!(&header.roots[0] == root);
                    Ok(())
                },
            ),
    )
    .try_collect::<Vec<_>>()
    .await?;

    Ok(())
}
