// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::collections::HashSet;
use std::error::Error as StdError;

use cid::Cid;
use encoding::Cbor;

use crate::Ipld;

// Traverses all Cid links, loading all unique values and using the callback function
// to interact with the data.
fn traverse_ipld_links<F>(
    walked: &mut HashSet<Cid>,
    load_block: &mut F,
    ipld: &Ipld,
) -> Result<(), Box<dyn StdError>>
where
    F: FnMut(Cid) -> Result<Vec<u8>, Box<dyn StdError>>,
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
            if cid.codec() == cid::DAG_CBOR {
                if !walked.insert(*cid) {
                    return Ok(());
                }
                let bytes = load_block(*cid)?;
                let ipld = Ipld::unmarshal_cbor(&bytes)?;
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
) -> Result<(), Box<dyn StdError>>
where
    F: FnMut(Cid) -> Result<Vec<u8>, Box<dyn StdError>>,
{
    if !walked.insert(root) {
        // Cid has already been traversed
        return Ok(());
    }
    if root.codec() != cid::DAG_CBOR {
        return Ok(());
    }

    let bytes = load_block(root)?;
    let ipld = Ipld::unmarshal_cbor(&bytes)?;

    traverse_ipld_links(walked, load_block, &ipld)?;

    Ok(())
}
