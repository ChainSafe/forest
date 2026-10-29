// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::db::PersistentStore;
use crate::utils::net::http_get;
use crate::{
    networks::{ACTOR_BUNDLES, ActorBundleInfo, NetworkChain},
    utils::db::car_stream::{CarBlock, CarStream},
};
use anyhow::Context as _;
use bytes::Bytes;
use cid::Cid;
use directories::ProjectDirs;
use futures::{Stream, TryStreamExt, stream::FuturesUnordered};
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::from_slice;
use std::mem::discriminant;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::{io::Cursor, path::Path};
use tracing::{info, warn};
use url::Url;

/// Tries to load the missing actor bundles to the blockstore. If the bundle is
/// not present, it will be downloaded.
pub async fn load_actor_bundles(
    db: &impl PersistentStore,
    network: &NetworkChain,
) -> anyhow::Result<()> {
    if let Some(bundle_path) = match std::env::var("FOREST_ACTOR_BUNDLE_PATH") {
        Ok(path) if !path.is_empty() => Some(path),
        _ => None,
    } {
        info!(
            "Loading actor bundle from {bundle_path} set by FOREST_ACTOR_BUNDLE_PATH environment variable"
        );
        load_actor_bundles_from_path(db, network, &bundle_path).await?;
    } else {
        load_actor_bundles_from_server(db, network, &ACTOR_BUNDLES).await?;
    }

    Ok(())
}

pub async fn load_actor_bundles_from_path(
    db: &impl PersistentStore,
    network: &NetworkChain,
    bundle_path: impl AsRef<Path>,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        bundle_path.as_ref().is_file(),
        "Bundle file not found at {}",
        bundle_path.as_ref().display()
    );
    let car_stream = CarStream::new_from_path(bundle_path.as_ref()).await?;

    // Validate the bundle
    let roots = car_stream.header_v1.roots.clone();
    for ActorBundleInfo {
        manifest, network, ..
    } in ACTOR_BUNDLES.iter().filter(|bundle| {
        // Comparing only the discriminant is enough. All devnets share the same
        // actor bundle.
        discriminant(network) == discriminant(&bundle.network)
    }) {
        anyhow::ensure!(
            roots.contains(manifest),
            "actor {manifest} for {network} is missing from {}, try regenerating the bundle with `forest-tool state-migration actor-bundle`",
            bundle_path.as_ref().display()
        );
    }

    let all_loaded = || {
        roots
            .iter()
            .try_for_each(|manifest| ensure_bundle_loaded(db, manifest))
    };
    if all_loaded().is_ok() {
        return Ok(());
    }
    persist_validated_blocks(db, car_stream).await?;
    all_loaded()
}

pub static ACTOR_BUNDLE_CACHE_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    let project_dir = ProjectDirs::from("com", "ChainSafe", "Forest");
    project_dir
        .map(|d| d.cache_dir().to_path_buf())
        .unwrap_or_else(std::env::temp_dir)
        .join("actor-bundles")
});

/// Loads the missing actor bundle, returns the CIDs of the loaded bundles.
pub async fn load_actor_bundles_from_server(
    db: &impl PersistentStore,
    network: &NetworkChain,
    bundles: &[ActorBundleInfo],
) -> anyhow::Result<Vec<Cid>> {
    FuturesUnordered::from_iter(
        bundles
            .iter()
            .filter(|bundle| {
                // Comparing only the discriminant is enough. All devnets share the same
                // actor bundle.
                discriminant(network) == discriminant(&bundle.network)
                    && ensure_bundle_loaded(db, &bundle.manifest).is_err()
            })
            .map(
                |ActorBundleInfo {
                     manifest: root,
                     url,
                     alt_url,
                     network,
                     version,
                 }| {
                    async move {
                        if let Err(e) = download_bundle(db, root, url).await {
                            warn!("failed to load bundle {network}-{version} from {url}, trying {alt_url}: {e:#}");
                            download_bundle(db, root, alt_url).await.with_context(|| {
                                format!("failed to load bundle {network}-{version} from {alt_url}")
                            })?;
                        }
                        Ok(*root)
                    }
                }
            ),
    )
    .try_collect::<Vec<_>>()
    .await
}

async fn download_bundle(
    db: &impl PersistentStore,
    manifest: &Cid,
    url: &Url,
) -> anyhow::Result<()> {
    let bytes = http_get(url).await?.bytes().await?;
    let stream = CarStream::new(Cursor::new(bytes)).await?;
    persist_validated_blocks(db, stream).await?;
    ensure_bundle_loaded(db, manifest)
}

async fn persist_validated_blocks(
    db: &impl PersistentStore,
    mut blocks: impl Stream<Item = std::io::Result<CarBlock>> + Unpin,
) -> anyhow::Result<()> {
    while let Some(block) = blocks.try_next().await? {
        block.validate()?;
        db.delete_keyed(&block.cid)?;
        db.put_keyed_persistent(&block.cid, &block.data)?;
    }
    Ok(())
}

/// Stored blocks are rehashed, since presence alone does not show a bundle is intact.
fn ensure_bundle_loaded(db: &impl Blockstore, manifest: &Cid) -> anyhow::Result<()> {
    let get_verified = |cid: &Cid| -> anyhow::Result<Bytes> {
        let block = CarBlock {
            cid: *cid,
            data: db
                .get(cid)?
                .with_context(|| format!("actor bundle {manifest} is missing block {cid}"))?
                .into(),
        };
        block
            .validate()
            .with_context(|| format!("stored block {cid} is corrupt"))?;
        Ok(block.data)
    };
    let (_, actor_list): (u32, Cid) = from_slice(&get_verified(manifest)?)?;
    for (_, code) in from_slice::<Vec<(String, Cid)>>(&get_verified(&actor_list)?)? {
        get_verified(&code)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::parity_db::ParityDb;
    use crate::db::parity_db_config::ParityDbConfig;
    use crate::shim::crypto::IPLD_RAW;
    use crate::utils::db::CborStoreExt as _;
    use crate::utils::multihash::prelude::*;
    use rstest::rstest;

    const INIT_CODE: &[u8] = b"init code";

    #[rstest]
    #[case::genuine(|db: &ParityDb, cid: &Cid| db.put_keyed_persistent(cid, INIT_CODE).unwrap(), true)]
    #[case::forged_code(|db: &ParityDb, cid: &Cid| db.put_keyed_persistent(cid, b"malicious").unwrap(), false)]
    #[case::forged_gc_copy(|db: &ParityDb, cid: &Cid| db.put_keyed(cid, b"malicious").unwrap(), false)]
    #[case::missing_code(|_: &ParityDb, _: &Cid| {}, false)]
    #[tokio::test]
    async fn loads_only_intact_bundles_and_repairs_on_reload(
        #[case] store_init_code: fn(&ParityDb, &Cid),
        #[case] intact: bool,
    ) {
        let dir = tempfile::tempdir().unwrap();
        // Reopening flushes ParityDb's commit queue, where writes to existing persistent keys are dropped.
        let open = || ParityDb::open(dir.path(), &ParityDbConfig::default()).unwrap();
        let init_code = CarBlock {
            cid: Cid::new_v1(IPLD_RAW, MultihashCode::Blake2b256.digest(INIT_CODE)),
            data: Bytes::from_static(INIT_CODE),
        };
        let manifest = {
            let db = open();
            let actor_list = db
                .put_cbor_default(&vec![("init".to_string(), init_code.cid)])
                .unwrap();
            store_init_code(&db, &init_code.cid);
            db.put_cbor_default(&(1u32, actor_list)).unwrap()
        };

        {
            let db = open();
            assert_eq!(ensure_bundle_loaded(&db, &manifest).is_ok(), intact);
            persist_validated_blocks(&db, futures::stream::iter([Ok(init_code)]))
                .await
                .unwrap();
        }

        ensure_bundle_loaded(&open(), &manifest).unwrap();
    }
}
