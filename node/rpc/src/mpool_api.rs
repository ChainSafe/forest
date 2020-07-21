// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::RpcState;

use address::Address;
use blocks::{BlockHeader, Tipset, TipsetKeys};
use blockstore::BlockStore;
use cid::json::CidJson;
use encoding::Cbor;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use message::Message;
use message::{
    signed_message::{self, json::SignedMessageJson},
    unsigned_message::json::UnsignedMessageJson,
    SignedMessage,
};
use message_pool::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::str::FromStr;
use wallet::KeyStore;

#[derive(Serialize, Deserialize)]
pub(crate) struct Pending {
    #[serde(rename = "Messages", with = "signed_message::json::vec")]
    pub msgs: Vec<SignedMessage>,
}

/// Estimate the gas price for an Address
pub(crate) async fn estimate_gas_price<DB, KS, MP>(
    data: Data<RpcState<DB, KS, MP>>,
    Params(params): Params<(u64, String, u64, TipsetKeys)>,
) -> Result<String, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    MP: Provider + Send + Sync + 'static,
{
    let (nblocks, sender_str, gas_limit, tsk) = params;
    let sender = Address::from_str(&sender_str)?;
    let price = data
        .mpool
        .estimate_gas_price(nblocks, sender, gas_limit, tsk)?;
    Ok(price.to_string())
}

/// get the sequence of given address in mpool
pub(crate) async fn get_sequence<DB, KS, MP>(
    data: Data<RpcState<DB, KS, MP>>,
    Params(params): Params<(String,)>,
) -> Result<u64, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    MP: Provider + Send + Sync + 'static,
{
    let (addr_str,) = params;
    let address = Address::from_str(&addr_str)?;
    let sequence = data.mpool.get_sequence(&address).await?;
    Ok(sequence)
}

/// Return Vec of pending messages in mpool
pub(crate) async fn pending<DB, KS, MP>(
    data: Data<RpcState<DB, KS, MP>>,
    Params(params): Params<(TipsetKeys,)>,
) -> Result<Pending, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
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
        return Ok(Pending { msgs: pending });
    }

    loop {
        if mpts.epoch() == ts.epoch() {
            if mpts == ts {
                return Ok(Pending { msgs: pending });
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
            return Ok(Pending { msgs: pending });
        }

        let mut headers: Vec<BlockHeader> = Vec::new();
        let parents = ts.parents().cids();
        for cid in parents {
            let block: BlockHeader = data
                .store
                .as_ref()
                .get(cid)?
                .ok_or("can't find block with cid")?;
            headers.push(block);
        }
        ts = Tipset::new(headers)?;
    }
}

/// Add SignedMessage to mpool, return msg CID
pub(crate) async fn push<DB, KS, MP>(
    data: Data<RpcState<DB, KS, MP>>,
    Params(params): Params<(SignedMessageJson,)>,
) -> Result<CidJson, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    MP: Provider + Send + Sync + 'static,
{
    let (SignedMessageJson(smsg),) = params;

    let cid = data.mpool.as_ref().push(smsg).await?;

    Ok(CidJson(cid))
}

/// Sign given UnsignedMessage and add it ot mpool, return SignedMessage
pub(crate) async fn push_message<DB, KS, MP>(
    data: Data<RpcState<DB, KS, MP>>,
    Params(params): Params<(UnsignedMessageJson,)>,
) -> Result<SignedMessageJson, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    MP: Provider + Send + Sync + 'static,
{
    let (UnsignedMessageJson(umsg),) = params;

    let from = umsg.from();
    let msg_cid = umsg.cid()?;

    let keystore = data.keystore.as_ref().write().await;
    let key = wallet::find_key(&from, &*keystore)?;
    let sig = wallet::sign(
        *key.key_info.key_type(),
        key.key_info.private_key(),
        msg_cid.to_bytes().as_slice(),
    )?;

    let smsg = SignedMessage::new_from_parts(umsg, sig)?;

    data.mpool.as_ref().push(smsg.clone()).await?;

    Ok(SignedMessageJson(smsg))
}
