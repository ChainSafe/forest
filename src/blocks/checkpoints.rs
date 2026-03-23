// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Provides utilities for efficiently locating the genesis block and known checkpoints
//! in the Filecoin blockchain by leveraging a list of precomputed, hash-chained block CIDs.
//! This avoids scanning millions of epochs, significantly speeding up chain traversal.

use crate::{
    blocks::{CachingBlockHeader, Tipset},
    networks::NetworkChain,
    shim::clock::ChainEpoch,
};
use ahash::HashMap;
use anyhow::Context as _;
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use serde_with::{DisplayFromStr, serde_as};
use std::sync::{LazyLock, OnceLock};

/// Holds mappings from chain epochs to block CIDs for each network.
#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct KnownBlocks {
    #[serde_as(as = "HashMap<_, DisplayFromStr>")]
    pub calibnet: HashMap<ChainEpoch, Cid>,
    #[serde_as(as = "HashMap<_, DisplayFromStr>")]
    pub mainnet: HashMap<ChainEpoch, Cid>,
}

/// Lazily loaded static instance of `KnownBlocks` from YAML.
/// Caches (`OnceLock`) are used to avoid recomputing known tipsets.
pub static KNOWN_BLOCKS: LazyLock<KnownBlocks> = LazyLock::new(|| {
    serde_yaml::from_str(include_str!("../../build/known_blocks.yaml")).expect("infallible")
});

/// Returns a cached, ascending-epoch list of known [`Tipset`]s for the given network.
pub fn known_tipsets(
    bs: &impl Blockstore,
    network: &NetworkChain,
) -> anyhow::Result<&'static Vec<Tipset>> {
    static CACHE_CALIBNET: OnceLock<Vec<Tipset>> = OnceLock::new();
    static CACHE_MAINNET: OnceLock<Vec<Tipset>> = OnceLock::new();
    let (cache, known_blocks) = match network {
        NetworkChain::Calibnet => (&CACHE_CALIBNET, &KNOWN_BLOCKS.calibnet),
        NetworkChain::Mainnet => (&CACHE_MAINNET, &KNOWN_BLOCKS.mainnet),
        _ => anyhow::bail!("unsupported network {network}"),
    };
    if let Some(v) = cache.get() {
        Ok(v)
    } else {
        let tipsets = known_blocks_to_known_tipsets(bs, known_blocks)?;
        _ = cache.set(tipsets);
        cache.get().context("infallible")
    }
}

fn known_blocks_to_known_tipsets(
    bs: &impl Blockstore,
    blocks: &HashMap<ChainEpoch, Cid>,
) -> anyhow::Result<Vec<Tipset>> {
    let mut tipsets: Vec<Tipset> = blocks
        .values()
        .map(|&b| block_cid_to_required_parent_tipset(bs, b))
        .try_collect()?;
    tipsets.sort_by_key(|ts| ts.epoch());
    Ok(tipsets)
}

fn block_cid_to_parent_tipset(bs: &impl Blockstore, block: Cid) -> anyhow::Result<Option<Tipset>> {
    if let Some(block) = CachingBlockHeader::load(bs, block)? {
        Tipset::load(bs, &block.parents)
    } else {
        Ok(None)
    }
}

fn block_cid_to_required_parent_tipset(bs: &impl Blockstore, block: Cid) -> anyhow::Result<Tipset> {
    block_cid_to_parent_tipset(bs, block)?
        .with_context(|| format!("failed to load parent tipset of block {block}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_known_blocks() {
        assert!(!KNOWN_BLOCKS.calibnet.is_empty());
        assert!(!KNOWN_BLOCKS.mainnet.is_empty());
    }
}
