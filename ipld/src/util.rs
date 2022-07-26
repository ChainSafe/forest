// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::HashSet;

use cid::Cid;
use fvm_ipld_encoding::from_slice;

use crate::Ipld;

// Traverses all Cid links, loading all unique values and using the callback function
// to interact with the data.
fn traverse_ipld_links<F>(
    walked: &mut HashSet<Cid>,
    load_block: &mut F,
    ipld: &Ipld,
) -> Result<(), anyhow::Error>
where
    F: FnMut(Cid) -> Result<Vec<u8>, anyhow::Error>,
{
    match ipld {
        Ipld::Map(m) => {
            for (_, v) in m.iter() {
                traverse_ipld_links(walked, load_block, v)?;
            }
        }
        Ipld::List(list) => {
            for v in list.iter() {
                traverse_ipld_links(walked, load_block, v)?;
            }
        }
        Ipld::Link(cid) => {
            // WASM blocks are stored as IPLD_RAW. They should be loaded but not traversed.
            if cid.codec() == fvm_shared::IPLD_RAW {
                if !walked.insert(*cid) {
                    return Ok(());
                }
                let _ = load_block(*cid)?;
            }
            if cid.codec() == fvm_ipld_encoding::DAG_CBOR {
                if !walked.insert(*cid) {
                    return Ok(());
                }
                let bytes = load_block(*cid)?;
                let ipld = from_slice(&bytes)?;
                traverse_ipld_links(walked, load_block, &ipld)?;
            }
        }
        _ => (),
    }
    Ok(())
}

// Load cids and call [traverse_ipld_links] to resolve recursively.
pub fn recurse_links<F>(
    walked: &mut HashSet<Cid>,
    root: Cid,
    load_block: &mut F,
) -> Result<(), anyhow::Error>
where
    F: FnMut(Cid) -> Result<Vec<u8>, anyhow::Error>,
{
    if !walked.insert(root) {
        // Cid has already been traversed
        return Ok(());
    }
    if root.codec() != fvm_ipld_encoding::DAG_CBOR {
        return Ok(());
    }

    let bytes = load_block(root)?;
    let ipld = from_slice(&bytes)?;

    traverse_ipld_links(walked, load_block, &ipld)?;

    Ok(())
}
