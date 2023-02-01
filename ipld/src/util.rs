// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashSet;
use cid::Cid;
use fvm_ipld_encoding::from_slice;
use std::future::Future;

use crate::Ipld;

/// Basic trait to abstract way the hashing details to the trait implementation.
pub trait InsertHash {
    /// Hashes the input and inserts its hash into the underlying collection.
    fn hash_and_insert(&mut self, data: &[u8]) -> bool;
}

impl InsertHash for HashSet<blake3::Hash> {
    fn hash_and_insert(&mut self, data: &[u8]) -> bool {
        self.insert(blake3::hash(data))
    }
}

/// Traverses all Cid links, hashing and loading all unique values and using the callback function
/// to interact with the data.
#[async_recursion::async_recursion]
async fn traverse_ipld_links_hash<F, T, H>(
    walked: &mut H,
    load_block: &mut F,
    ipld: &Ipld,
) -> Result<(), anyhow::Error>
where
    F: FnMut(Cid) -> T + Send,
    T: Future<Output = Result<Vec<u8>, anyhow::Error>> + Send,
    H: InsertHash + Send,
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
                if !walked.hash_and_insert(&cid.to_bytes()) {
                    return Ok(());
                }
                let _ = load_block(*cid).await?;
            }
            if cid.codec() == fvm_ipld_encoding::DAG_CBOR {
                if !walked.hash_and_insert(&cid.to_bytes()) {
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

/// Load and hash cids and resolve recursively.
pub async fn recurse_links_hash<F, T, H>(
    walked: &mut H,
    root: Cid,
    load_block: &mut F,
) -> Result<(), anyhow::Error>
where
    F: FnMut(Cid) -> T + Send,
    T: Future<Output = Result<Vec<u8>, anyhow::Error>> + Send,
    H: InsertHash + Send,
{
    if !walked.hash_and_insert(&root.to_bytes()) {
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
