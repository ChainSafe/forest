// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::gas_api::estimate_message_gas;
use forest_beacon::Beacon;
use forest_blocks::TipsetKeys;
use forest_db::Store;
use forest_json::cid::{vec::CidJsonVec, CidJson};
use forest_message::message::json::MessageJson;
use forest_message::{signed_message::json::SignedMessageJson, SignedMessage};
use forest_rpc_api::data_types::RPCState;
use forest_rpc_api::mpool_api::*;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::Cbor;
use fvm_shared::address::{Address, Protocol};

use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use std::str::FromStr;
use std::{collections::HashSet, convert::TryFrom};

/// Estimate the gas price for an Address
pub(crate) async fn estimate_gas_premium<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<MpoolEstimateGasPriceParams>,
) -> Result<MpoolEstimateGasPriceResult, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (nblocks, sender_str, gas_limit, tsk) = params;
    let sender = Address::from_str(&sender_str)?;
    let price = data
        .mpool
        .estimate_gas_premium(nblocks, sender, gas_limit, tsk)?;
    Ok(price.to_string())
}

/// get the sequence of given address in `mpool`
pub(crate) async fn mpool_get_sequence<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<MpoolGetNonceParams>,
) -> Result<MpoolGetNonceResult, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (addr_str,) = params;
    let address = Address::from_str(&addr_str)?;
    let sequence = data.mpool.get_sequence(&address).await?;
    Ok(sequence)
}

/// Return `Vec` of pending messages in `mpool`
pub(crate) async fn mpool_pending<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<MpoolPendingParams>,
) -> Result<MpoolPendingResult, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
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
    B: Beacon + Send + Sync + 'static,
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
    B: Beacon + Send + Sync + 'static,
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

pub(crate) async fn mpool_select<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<MpoolSelectParams>,
) -> Result<MpoolSelectResult, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (tsk, q) = params;
    let ts = data.chain_store.tipset_from_keys(&tsk.into()).await?;

    Ok(data
        .mpool
        .select_messages(ts.as_ref(), q)
        .await
        .map_err(|e| format!("Failed to select messages: {:?}", e))?
        .into_iter()
        .map(|e| e.into())
        .collect())
}
