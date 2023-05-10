// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::anyhow;
use cid::Cid;
use forest_networks::{ChainConfig, Height};
use forest_shim::clock::ChainEpoch;
use fvm_ipld_blockstore::Blockstore;

/// Runs the migration for `NV17`. Returns the new state root.
pub fn run_migration<DB>(
    chain_config: &ChainConfig,
    blockstore: &DB,
    state: &Cid,
    epoch: ChainEpoch,
) -> anyhow::Result<Cid>
where
    DB: 'static + Blockstore + Clone + Send + Sync,
{
    let new_manifest_cid = chain_config
        .height_infos
        .get(Height::Shark as usize)
        .ok_or_else(|| anyhow!("no height info for network version NV17"))?
        .bundle
        .as_ref()
        .ok_or_else(|| anyhow!("no bundle info for network version NV17"))?
        .manifest;

    blockstore.get(&new_manifest_cid)?.ok_or_else(|| {
        anyhow!(
            "manifest for network version NV17 not found in blockstore: {}",
            new_manifest_cid
        )
    })?;

    // Add migration specification verification
    // let verifier = Arc::new(Verifier::default());

    todo!()
}
