// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::State;
use crate::chain_api::chain_get_tipset;

use address::Address;
use blocks::{TipsetKeys, BlockHeader, Tipset};
use blockstore::BlockStore;
use cid::{json::CidJson, Cid};
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use message::{
    signed_message::{self, json::{SignedMessageJson, vec}},
    unsigned_message::{self, json::UnsignedMessageJson},
    SignedMessage, UnsignedMessage,
};
use message_pool::*;
use num_bigint::BigInt;
use serde::{Deserialize, Serialize};
use message::Message;
use crypto::Signature;
use std::collections::HashSet;
use encoding::Cbor;

#[derive(Serialize, Deserialize)]
pub(crate) struct Pending {
    #[serde(rename = "Messages", with = "signed_message::json::vec")]
    pub msgs: Vec<SignedMessage>
}

/// Estimate the gas price for an Address
pub(crate) async fn estimate_gas_price<DB, MP>(
    data: Data<State<DB, MP>>,
    Params(params): Params<(u64, Address, u64, TipsetKeys)>,
) -> Result<BigInt, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    MP: Provider + Send + Sync + 'static,
{
    let (nblocks, sender, gas_limit, tsk) = params;
    let price = data
        .mpool
        .estimate_gas_price(nblocks, sender, gas_limit, tsk)?;
    Ok(price)
}

/// get the sequence of given address in mpool
pub(crate) async fn get_sequence<DB, MP>(
    data: Data<State<DB, MP>>,
    Params(params): Params<(Address,)>,
) -> Result<u64, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    MP: Provider + Send + Sync + 'static,
{
    let (address,) = params;
    let sequence = data.mpool.get_sequence(&address).await?;
    Ok(sequence)
}

/// Return Vec of pending messages in mpool
pub(crate) async fn pending<DB, MP>(
    data: Data<State<DB, MP>>,
    Params(params): Params<(TipsetKeys,)>,
) -> Result<Pending, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    MP: Provider + Send + Sync + 'static,
{
    let (tsk,) = params;
    let mut ts = chain::tipset_from_keys(data.store.as_ref(), &tsk)?;

    let (mut pending, mpts) = data.mpool.pending().await?;

    let mut have_cids = HashSet::new();
    for item in pending.iter() {
        have_cids.insert(item.cid()?);
    }

    if mpts.epoch() > ts.epoch() {
        return Ok(Pending{ msgs: pending });
    }

    loop {
        if mpts.epoch() == ts.epoch() {
            if mpts == ts {
                return Ok(Pending{ msgs: pending });
            }

            // mpts has different blocks than ts
            let have = data.mpool.as_ref().messages_for_blocks(ts.blocks()).await?;

            for sm in have {
                have_cids.insert(sm.cid()?);
            }
        }

        let msgs = data.mpool.as_ref().messages_for_blocks(ts.blocks()).await?;

        for m in msgs {
            if have_cids.contains(&m.cid()?) {
                continue;
            }

            have_cids.insert(m.cid()?);
            pending.push(m);
        }

        if mpts.epoch() >= ts.epoch() {
            return Ok(Pending{ msgs: pending });
        }

        let mut headers: Vec<BlockHeader> = Vec::new();
        let parents = ts.parents().cids();
        for cid in parents {
            let block: BlockHeader = data.store.as_ref().get(cid)?.ok_or("can't find block with cid")?;
            headers.push(block);
        }
        ts = Tipset::new(headers)?;


    }
}

/// Add SignedMessage to mpool, return msg CID
pub(crate) async fn push<DB, MP>(
    data: Data<State<DB, MP>>,
    Params(params): Params<(SignedMessageJson,)>,
) -> Result<CidJson, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    MP: Provider + Send + Sync + 'static,
{
    let (SignedMessageJson(smsg),) = params;

    let cid = data.mpool.as_ref().push(smsg).await?;

    Ok(CidJson(cid))
}

/// Sign given UnsignedMessage and add it ot mpool, return SignedMessage
pub(crate) async fn push_message<DB, MP>(
    data: Data<State<DB, MP>>,
    Params(params): Params<(UnsignedMessageJson,)>,
) -> Result<SignedMessageJson, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    MP: Provider + Send + Sync + 'static,
{
    unimplemented!();
    // let (UnsignedMessageJson(umsg),) = params;
    //
    // let from = umsg.from();
    // let msg_cid = umsg.cid()?;
    //
    // let keystore = data.keystore.as_ref().write().await;
    // let key = wallet::find_key(&from, &*keystore)?;
    // let sig = wallet::sign(
    //     *key.key_info.key_type(),
    //     key.key_info.private_key(),
    //     msg_cid.to_bytes().as_slice(),
    // )?;
    //
    // let smsg = SignedMessage::new_from_parts(umsg, sig);
    //
    // data.mpool.as_ref().push(smsg.clone()).await?;
    //
    // Ok(SignedMessageJson(smsg))
}
