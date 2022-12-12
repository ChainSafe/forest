// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::gas_api::estimate_message_gas;
use forest_beacon::Beacon;
use forest_blocks::TipsetKeys;
use forest_db::Store;
use forest_json::cid::{vec::CidJsonVec, CidJson};
use forest_json::message::json::MessageJson;
use forest_json::signed_message::json::SignedMessageJson;
use forest_message::SignedMessage;
use forest_rpc_api::data_types::RPCState;
use forest_rpc_api::mpool_api::*;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::Cbor;
use fvm_shared::address::Protocol;

use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use std::{collections::HashSet, convert::TryFrom};

/// Return `Vec` of pending messages in `mpool`
pub(crate) async fn mpool_pending<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<MpoolPendingParams>,
) -> Result<MpoolPendingResult, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (CidJsonVec(cid_vec),) = params;
    let tsk = TipsetKeys::new(cid_vec);
    let mut ts = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&tsk)
        .await?;

    let (mut pending, mpts) = data.mpool.pending().await?;

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
            return Ok(pending);
        }

        ts = data
            .state_manager
            .chain_store()
            .tipset_from_keys(ts.parents())
            .await?;
    }
}

/// Add `SignedMessage` to `mpool`, return message CID
pub(crate) async fn mpool_push<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<MpoolPushParams>,
) -> Result<MpoolPushResult, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (SignedMessageJson(smsg),) = params;

    let cid = data.mpool.as_ref().push(smsg).await?;

    Ok(CidJson(cid))
}

/// Sign given `UnsignedMessage` and add it to `mpool`, return `SignedMessage`
pub(crate) async fn mpool_push_message<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<MpoolPushMessageParams>,
) -> Result<MpoolPushMessageResult, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (MessageJson(umsg), spec) = params;

    let from = umsg.from;

    let mut keystore = data.keystore.as_ref().write().await;
    let heaviest_tipset = data
        .state_manager
        .chain_store()
        .heaviest_tipset()
        .await
        .ok_or_else(|| "Could not get heaviest tipset".to_string())?;
    let key_addr = data
        .state_manager
        .resolve_to_key_addr(&from, &heaviest_tipset)
        .await?;

    if umsg.sequence != 0 {
        return Err(
            "Expected nonce for MpoolPushMessage is 0, and will be calculated for you.".into(),
        );
    }
    let mut umsg = estimate_message_gas::<DB, B>(&data, umsg, spec, Default::default()).await?;
    if umsg.gas_premium > umsg.gas_fee_cap {
        return Err("After estimation, gas premium is greater than gas fee cap".into());
    }

    if from.protocol() == Protocol::ID {
        umsg.from = key_addr;
    }
    let nonce = data.mpool.get_sequence(&from).await?;
    umsg.sequence = nonce;
    let key = forest_key_management::Key::try_from(forest_key_management::try_find(
        &key_addr,
        &mut keystore,
    )?)?;
    let sig = forest_key_management::sign(
        *key.key_info.key_type(),
        key.key_info.private_key(),
        umsg.cid().unwrap().to_bytes().as_slice(),
    )?;

    let smsg = SignedMessage::new_from_parts(umsg, sig)?;

    data.mpool.as_ref().push(smsg.clone()).await?;

    Ok(SignedMessageJson(smsg))
}
