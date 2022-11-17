// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use libipld::{prelude::*, store::StoreParams, Ipld};

pub(super) fn bitswap_missing_blocks<BS: Blockstore, P: StoreParams>(
    bs: &mut BS,
    cid: &Cid,
) -> anyhow::Result<Vec<Cid>>
where
    Ipld: References<<P as StoreParams>::Codecs>,
{
    let mut stack = vec![*cid];
    let mut missing = vec![];
    while let Some(cid) = stack.pop() {
        if let Some(data) = bs.get(&cid)? {
            let block = libipld::Block::<P>::new_unchecked(cid, data);
            block.references(&mut stack)?;
        } else {
            missing.push(cid);
        }
    }
    Ok(missing)
}
