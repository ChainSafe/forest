// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::Context;
use cid::Cid;
use forest_utils::db::BlockstoreExt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::DAG_CBOR;
use libipld_core::ipld::Ipld;

/// Resolves link to recursively resolved [`Ipld`] with no hash links.
pub fn resolve_cids_recursive<BS>(
    bs: &BS,
    cid: &Cid,
    depth: Option<u64>,
) -> Result<Ipld, anyhow::Error>
where
    BS: Blockstore,
{
    let mut ipld = bs
        .get_obj(cid)?
        .context("Cid does not exist in blockstore")?;

    resolve_ipld(bs, &mut ipld, depth)?;

    Ok(ipld)
}

/// Resolves [`Ipld`] links recursively, building an [`Ipld`] structure with no hash links.
fn resolve_ipld<BS>(bs: &BS, ipld: &mut Ipld, mut depth: Option<u64>) -> Result<(), anyhow::Error>
where
    BS: Blockstore,
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
                if let Some(mut x) = bs.get_obj(cid)? {
                    resolve_ipld(bs, &mut x, depth)?;
                    *ipld = x;
                }
            }
        }
        _ => (),
    }
    Ok(())
}
