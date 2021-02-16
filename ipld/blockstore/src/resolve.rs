// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::BlockStore;
use cid::{Cid, DAG_CBOR};
use forest_ipld::Ipld;
use std::error::Error as StdError;

/// Resolves link to recursively resolved [Ipld] with no hash links.
pub fn resolve_cids_recursive<BS>(
    bs: &BS,
    cid: &Cid,
    depth: Option<u64>,
) -> Result<Ipld, Box<dyn StdError>>
where
    BS: BlockStore,
{
    let mut ipld = bs.get(cid)?.ok_or("Cid does not exist in blockstore")?;

    resolve_ipld(bs, &mut ipld, depth)?;

    Ok(ipld)
}

/// Resolves [Ipld] links recursively, building an [Ipld] structure with no hash links.
pub fn resolve_ipld<BS>(
    bs: &BS,
    ipld: &mut Ipld,
    mut depth: Option<u64>,
) -> Result<(), Box<dyn StdError>>
where
    BS: BlockStore,
{
    if let Some(dep) = depth.as_mut() {
        if *dep == 0 {
            return Ok(());
        }
        *dep -= 1;
    }
    match ipld {
        Ipld::Map(m) => {
            for (_, v) in m.iter_mut() {
                resolve_ipld(bs, v, depth)?;
            }
        }
        Ipld::List(list) => {
            for v in list.iter_mut() {
                resolve_ipld(bs, v, depth)?;
            }
        }
        Ipld::Link(cid) => {
            if cid.codec() == DAG_CBOR {
                if let Some(x) = bs.get(cid)? {
                    *ipld = x;
                }
            }
        }
        _ => (),
    }
    Ok(())
}
