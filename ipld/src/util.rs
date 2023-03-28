// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{collections::VecDeque, future::Future};

use cid::Cid;
use forest_blocks::{BlockHeader, Tipset};
use fvm_ipld_encoding::{from_slice, Cbor};

use crate::{CidHashSet, Ipld};

/// Traverses all Cid links, hashing and loading all unique values and using the
/// callback function to interact with the data.
#[async_recursion::async_recursion]
async fn traverse_ipld_links_hash<F, T>(
    walked: &mut CidHashSet,
    load_block: &mut F,
    ipld: &Ipld,
) -> Result<(), anyhow::Error>
where
    F: FnMut(Cid) -> T + Send,
    T: Future<Output = Result<Vec<u8>, anyhow::Error>> + Send,
{
    match ipld {
        Ipld::Map(m) => {
            for (_, v) in m.iter() {
                traverse_ipld_links_hash(walked, load_block, v).await?;
            }
        }
        Ipld::List(list) => {
            for v in list.iter() {
                traverse_ipld_links_hash(walked, load_block, v).await?;
            }
        }
        Ipld::Link(cid) => {
            // WASM blocks are stored as IPLD_RAW. They should be loaded but not traversed.
            if cid.codec() == fvm_shared::IPLD_RAW {
                if !walked.insert(cid) {
                    return Ok(());
                }
                let _ = load_block(*cid).await?;
            }
            if cid.codec() == fvm_ipld_encoding::DAG_CBOR {
                if !walked.insert(cid) {
                    return Ok(());
                }
                let bytes = load_block(*cid).await?;
                let ipld = from_slice(&bytes)?;
                traverse_ipld_links_hash(walked, load_block, &ipld).await?;
            }
        }
        _ => (),
    }
    Ok(())
}

/// Load and hash CIDs and resolve recursively.
pub async fn recurse_links_hash<F, T>(
    walked: &mut CidHashSet,
    root: Cid,
    load_block: &mut F,
) -> Result<(), anyhow::Error>
where
    F: FnMut(Cid) -> T + Send,
    T: Future<Output = Result<Vec<u8>, anyhow::Error>> + Send,
{
    if !walked.insert(&root) {
        // Cid has already been traversed
        return Ok(());
    }
    if root.codec() != fvm_ipld_encoding::DAG_CBOR {
        return Ok(());
    }

    let bytes = load_block(root).await?;
    let ipld = from_slice(&bytes)?;

    traverse_ipld_links_hash(walked, load_block, &ipld).await?;

    Ok(())
}

pub const DEFAULT_RECENT_STATE_ROOTS: i64 = 2000;

/// Walks over tipset and state data and loads all blocks not yet seen.
/// This is tracked based on the callback function loading blocks.
pub async fn walk_snapshot<F, T>(
    tipset: &Tipset,
    recent_roots: i64,
    mut load_block: F,
) -> anyhow::Result<()>
where
    F: FnMut(Cid) -> T + Send,
    T: Future<Output = anyhow::Result<Vec<u8>>> + Send,
{
    let mut seen = CidHashSet::default();
    let mut blocks_to_walk: VecDeque<Cid> = tipset.cids().to_vec().into();
    let mut current_min_height = tipset.epoch();
    let incl_roots_epoch = tipset.epoch() - recent_roots;

    while let Some(next) = blocks_to_walk.pop_front() {
        if !seen.insert(&next) {
            continue;
        }

        if !should_save_block_to_snapshot(&next) {
            continue;
        }

        let data = load_block(next).await?;

        let h = BlockHeader::unmarshal_cbor(&data)?;

        if current_min_height > h.epoch() {
            current_min_height = h.epoch();
        }

        if h.epoch() > incl_roots_epoch {
            recurse_links_hash(&mut seen, *h.messages(), &mut load_block).await?;
        }

        if h.epoch() > 0 {
            for p in h.parents().cids() {
                blocks_to_walk.push_back(*p);
            }
        } else {
            for p in h.parents().cids() {
                load_block(*p).await?;
            }
        }

        if h.epoch() == 0 || h.epoch() > incl_roots_epoch {
            recurse_links_hash(&mut seen, *h.state_root(), &mut load_block).await?;
        }
    }

    Ok(())
}

fn should_save_block_to_snapshot(cid: &Cid) -> bool {
    // Don't include identity CIDs.
    // We only include raw and dagcbor, for now.
    // Raw for "code" CIDs.
    if cid.hash().code() == u64::from(cid::multihash::Code::Identity) {
        false
    } else {
        matches!(
            cid.codec(),
            fvm_shared::IPLD_RAW | fvm_ipld_encoding::DAG_CBOR
        )
    }
}
