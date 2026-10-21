// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::db::PersistentStore;
use crate::shim::machine::BuiltinActorManifest;
use crate::utils::net::http_get;
use crate::{
    networks::{ACTOR_BUNDLES, ActorBundleInfo, NetworkChain},
    utils::db::car_stream::{CarBlock, CarStream},
};
use ahash::HashSet;
use cid::Cid;
use directories::ProjectDirs;
use futures::{Stream, TryStreamExt, stream::FuturesUnordered};
use fvm_ipld_blockstore::Blockstore;
use std::mem::discriminant;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::{io::Cursor, path::Path};
use tracing::{info, warn};

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
    let roots: HashSet<Cid> = car_stream.header_v1.roots.iter().copied().collect();
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

    persist_validated_blocks(db, car_stream).await?;
    for manifest in &roots {
        ensure_bundle_loaded(db, manifest)?;
    }

    Ok(())
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
                ensure_bundle_loaded(db, &bundle.manifest).is_err() &&
                // Comparing only the discriminant is enough. All devnets share the same
                // actor bundle.
                discriminant(network) == discriminant(&bundle.network)
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
                        let response = if let Ok(response) =
                            http_get(url).await
                        {
                            response
                        } else {
                            warn!("failed to download bundle {network}-{version} from primary URL, trying alternative URL");
                            http_get(alt_url).await?
                        };

                        let bytes = response.bytes().await?;
                        let stream = CarStream::new(Cursor::new(bytes)).await?;
                        anyhow::ensure!(stream.header_v1.roots.len() == 1);
                        anyhow::ensure!(stream.header_v1.roots.first() == root);
                        persist_validated_blocks(db, stream).await?;
                        ensure_bundle_loaded(db, root)?;
                        Ok(*root)
                    }
                }
            ),
    )
    .try_collect::<Vec<_>>()
    .await
}

async fn persist_validated_blocks(
    db: &impl PersistentStore,
    mut blocks: impl Stream<Item = std::io::Result<CarBlock>> + Unpin,
) -> anyhow::Result<()> {
    while let Some(block) = blocks.try_next().await? {
        block.validate()?;
        db.put_keyed_persistent(&block.cid, &block.data)?;
    }
    Ok(())
}

/// A bundle counts as loaded only if its manifest decodes and all of its actor code is in `db`, so a partial or interrupted load is retried.
fn ensure_bundle_loaded(db: &impl Blockstore, manifest: &Cid) -> anyhow::Result<()> {
    for (actor, code) in BuiltinActorManifest::load_manifest(db, manifest)?.builtin_actors() {
        anyhow::ensure!(
            db.has(&code)?,
            "actor bundle {manifest} is missing code {code} for {}",
            actor.name()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::MemoryDB;
    use crate::shim::crypto::IPLD_RAW;
    use crate::utils::cid::CidCborExt as _;
    use crate::utils::multihash::prelude::*;
    use bytes::Bytes;
    use rstest::rstest;

    fn raw_block(data: &'static [u8]) -> CarBlock {
        CarBlock {
            cid: Cid::new_v1(IPLD_RAW, MultihashCode::Blake2b256.digest(data)),
            data: Bytes::from_static(data),
        }
    }

    fn cbor_block(obj: &impl serde::Serialize) -> CarBlock {
        CarBlock {
            cid: Cid::from_cbor_blake2b256(obj).unwrap(),
            data: fvm_ipld_encoding::to_vec(obj).unwrap().into(),
        }
    }

    #[rstest]
    #[case::genuine(|_: &mut Vec<CarBlock>| {}, true)]
    #[case::tampered_code(|blocks: &mut Vec<CarBlock>| blocks.last_mut().unwrap().data = Bytes::from_static(b"malicious"), false)]
    #[case::missing_code(|blocks: &mut Vec<CarBlock>| { blocks.pop(); }, false)]
    #[tokio::test]
    async fn bundle_is_loaded_only_when_intact(
        #[case] corrupt: fn(&mut Vec<CarBlock>),
        #[case] intact: bool,
    ) {
        let (system, init) = (raw_block(b"system code"), raw_block(b"init code"));
        let actor_list = cbor_block(&vec![
            ("system".to_string(), system.cid),
            ("init".to_string(), init.cid),
        ]);
        let manifest = cbor_block(&(1u32, actor_list.cid));
        let manifest_cid = manifest.cid;
        let mut blocks = vec![manifest, actor_list, system, init];
        corrupt(&mut blocks);

        let db = MemoryDB::default();
        let _ =
            persist_validated_blocks(&db, futures::stream::iter(blocks.into_iter().map(Ok))).await;

        assert_eq!(ensure_bundle_loaded(&db, &manifest_cid).is_ok(), intact);
    }
}
