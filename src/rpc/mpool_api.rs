// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use std::convert::TryFrom;

use crate::blocks::TipsetKeys;
use crate::json::{
    cid::{vec::CidJsonVec, CidJson},
    message::json::MessageJson,
    signed_message::json::SignedMessageJson,
};
use crate::message::SignedMessage;
use crate::rpc_api::{data_types::RPCState, mpool_api::*};
use crate::shim::address::Protocol;
use ahash::{HashSet, HashSetExt};
use fvm_ipld_blockstore::Blockstore;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};

use super::gas_api::estimate_message_gas;

/// Return `Vec` of pending messages in `mpool`
pub(in crate::rpc) async fn mpool_pending<DB>(
    data: Data<RPCState<DB>>,
    Params(params): Params<MpoolPendingParams>,
) -> Result<MpoolPendingResult, JsonRpcError>
where
    DB: Blockstore + Clone + Send + Sync + 'static,
{
    let (CidJsonVec(cid_vec),) = params;
    let tsk = TipsetKeys::new(cid_vec);
    let mut ts = data.state_manager.chain_store().tipset_from_keys(&tsk)?;

    let (mut pending, mpts) = data.mpool.pending()?;

    let mut have_cids = HashSet::new();
    for item in pending.iter() {
        have_cids.insert(item.cid()?);
    }

    if mpts.epoch() > ts.epoch() {
        return Ok(pending);
    }

    loop {
        if mpts.epoch() == ts.epoch() {
            if mpts == ts {
                return Ok(pending);
            }

            // mpts has different blocks than ts
            let have = data.mpool.as_ref().messages_for_blocks(ts.blocks())?;

            for sm in have {
                have_cids.insert(sm.cid()?);
            }
        }

        let msgs = data.mpool.as_ref().messages_for_blocks(ts.blocks())?;

        for m in msgs {
            if have_cids.contains(&m.cid()?) {
                continue;
            }

            have_cids.insert(m.cid()?);
            pending.push(m);
        }

        if mpts.epoch() >= ts.epoch() {
            return Ok(pending);
        }

        ts = data
            .state_manager
            .chain_store()
            .tipset_from_keys(ts.parents())?;
    }
}

/// Add `SignedMessage` to `mpool`, return message CID
pub(in crate::rpc) async fn mpool_push<DB>(
    data: Data<RPCState<DB>>,
    Params(params): Params<MpoolPushParams>,
) -> Result<MpoolPushResult, JsonRpcError>
where
    DB: Blockstore + Clone + Send + Sync + 'static,
{
    let (SignedMessageJson(smsg),) = params;

    let cid = data.mpool.as_ref().push(smsg).await?;

    Ok(CidJson(cid))
}

/// Sign given `UnsignedMessage` and add it to `mpool`, return `SignedMessage`
pub(in crate::rpc) async fn mpool_push_message<DB>(
    data: Data<RPCState<DB>>,
    Params(params): Params<MpoolPushMessageParams>,
) -> Result<MpoolPushMessageResult, JsonRpcError>
where
    DB: Blockstore + Clone + Send + Sync + 'static,
{
    let (MessageJson(umsg), spec) = params;

    let from = umsg.from;

    let mut keystore = data.keystore.as_ref().write().await;
    let heaviest_tipset = data.state_manager.chain_store().heaviest_tipset();
    let key_addr = data
        .state_manager
        .resolve_to_key_addr(&from, &heaviest_tipset)
        .await?;

    if umsg.sequence != 0 {
        return Err(
            "Expected nonce for MpoolPushMessage is 0, and will be calculated for you.".into(),
        );
    }
    let mut umsg = estimate_message_gas::<DB>(&data, umsg, spec, Default::default()).await?;
    if umsg.gas_premium > umsg.gas_fee_cap {
        return Err("After estimation, gas premium is greater than gas fee cap".into());
    }

    if from.protocol() == Protocol::ID {
        umsg.from = key_addr;
    }
    let nonce = data.mpool.get_sequence(&from)?;
    umsg.sequence = nonce;
    let key = crate::key_management::Key::try_from(crate::key_management::try_find(
        &key_addr,
        &mut keystore,
    )?)?;
    let sig = crate::key_management::sign(
        *key.key_info.key_type(),
        key.key_info.private_key(),
        umsg.cid().unwrap().to_bytes().as_slice(),
    )?;

    let smsg = SignedMessage::new_from_parts(umsg, sig)?;

    data.mpool.as_ref().push(smsg.clone()).await?;

    Ok(SignedMessageJson(smsg))
}
