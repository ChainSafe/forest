// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    networks::{generate_actor_bundle, ActorBundleInfo},
    utils::db::car_util::load_car,
};
use anyhow::Context as _;
use fvm_ipld_blockstore::Blockstore;
use tracing::info;

/// Tries to load the actor bundle from the specified location to the blockstore. If the bundle is
/// not present, it will be generated.
pub async fn load_actor_bundles(db: &impl Blockstore) -> anyhow::Result<()> {
    if !are_bundles_present(db, &crate::networks::ACTOR_BUNDLES) {
        info!("Generating actor bundle");
        let bundle = tempfile::Builder::new()
            .prefix("forest-actor-bundle")
            .suffix(".car.zst")
            .tempfile()?;
        generate_actor_bundle(bundle.path()).await?;

        let bundle_reader = tokio::io::BufReader::new(tokio::fs::File::open(bundle.path()).await?);

        load_car(db, bundle_reader)
            .await
            .context("failed to load actor bundle")?;
    } else {
        info!("Actor bundles already present in the blockstore. Skipping generation");
    }

    Ok(())
}

/// Checks if all the actor bundles are present in the blockstore.
fn are_bundles_present(db: &impl Blockstore, required_bundles: &[ActorBundleInfo]) -> bool {
    required_bundles
        .iter()
        .map(|bundle| bundle.manifest)
        .all(|manifest| db.has(&manifest).unwrap_or(false))
}

#[cfg(test)]
mod tests {
    use crate::utils::cid::CidCborExt;
    use cid::Cid;
    use url::Url;

    use super::*;
    #[test]
    fn are_bundles_present_empty() {
        let db = fvm_ipld_blockstore::MemoryBlockstore::default();
        let required_bundles = vec![];

        assert!(are_bundles_present(&db, &required_bundles));
    }

    #[test]
    fn are_bundles_present_non_empty() {
        let db = fvm_ipld_blockstore::MemoryBlockstore::default();
        let required_bundles = vec![
            ActorBundleInfo {
                url: Url::parse("https://chainsafe.io").unwrap(),
                alt_url: Url::parse("https://chainsafe.io").unwrap(),
                manifest: Cid::from_cbor_blake2b256(&1).unwrap(),
            },
            ActorBundleInfo {
                url: Url::parse("https://filecoin.io").unwrap(),
                alt_url: Url::parse("https://filecoin.io").unwrap(),
                manifest: Cid::from_cbor_blake2b256(&2).unwrap(),
            },
        ];

        db.put_keyed(&Cid::from_cbor_blake2b256(&1).unwrap(), &[])
            .unwrap();

        assert!(!are_bundles_present(&db, &required_bundles));

        db.put_keyed(&Cid::from_cbor_blake2b256(&2).unwrap(), &[])
            .unwrap();

        assert!(are_bundles_present(&db, &required_bundles));
    }
}
