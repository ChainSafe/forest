// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::BlockStore;
use crate::BlockStoreExt;
use cid::{Cid, DAG_CBOR};
use forest_ipld::Ipld;

/// Resolves link to recursively resolved [Ipld] with no hash links.
pub fn resolve_cids_recursive<BS>(
    bs: &BS,
    cid: &Cid,
    depth: Option<u64>,
) -> Result<Ipld, anyhow::Error>
where
    BS: BlockStore,
{
    let mut ipld = bs
        .get_obj(cid)?
        .ok_or_else(|| anyhow::anyhow!("Cid does not exist in blockstore"))?;

    resolve_ipld(bs, &mut ipld, depth)?;

    Ok(ipld)
}

/// Resolves [Ipld] links recursively, building an [Ipld] structure with no hash links.
pub fn resolve_ipld<BS>(
    bs: &BS,
    ipld: &mut Ipld,
    mut depth: Option<u64>,
) -> Result<(), anyhow::Error>
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
                if let Some(x) = bs.get_obj(cid)? {
                    *ipld = x;
                }
            }
        }
        _ => (),
    }
    Ok(())
}
