// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{collections::HashSet, future::Future};

use cid::Cid;
use fvm_ipld_encoding::from_slice;

use crate::Ipld;

// Traverses all Cid links, loading all unique values and using the callback function
// to interact with the data.
#[async_recursion::async_recursion]
async fn traverse_ipld_links<F, T>(
    walked: &mut HashSet<Cid>,
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
                traverse_ipld_links(walked, load_block, v).await?;
            }
        }
        Ipld::List(list) => {
            for v in list.iter() {
                traverse_ipld_links(walked, load_block, v).await?;
            }
        }
        Ipld::Link(cid) => {
            // WASM blocks are stored as IPLD_RAW. They should be loaded but not traversed.
            if cid.codec() == fvm_shared::IPLD_RAW {
                if !walked.insert(*cid) {
                    return Ok(());
                }
                let _ = load_block(*cid).await?;
            }
            if cid.codec() == fvm_ipld_encoding::DAG_CBOR {
                if !walked.insert(*cid) {
                    return Ok(());
                }
                let bytes = load_block(*cid).await?;
                let ipld = from_slice(&bytes)?;
                traverse_ipld_links(walked, load_block, &ipld).await?;
            }
        }
        _ => (),
    }
    Ok(())
}

// Load cids and call [traverse_ipld_links] to resolve recursively.
pub async fn recurse_links<F, T>(
    walked: &mut HashSet<Cid>,
    root: Cid,
    load_block: &mut F,
) -> Result<(), anyhow::Error>
where
    F: FnMut(Cid) -> T + Send,
    T: Future<Output = Result<Vec<u8>, anyhow::Error>> + Send,
{
    if !walked.insert(root) {
        // Cid has already been traversed
        return Ok(());
    }
    if root.codec() != fvm_ipld_encoding::DAG_CBOR {
        return Ok(());
    }

    let bytes = load_block(root).await?;
    let ipld = from_slice(&bytes)?;

    traverse_ipld_links(walked, load_block, &ipld).await?;

    Ok(())
}
