// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use jsonrpc_v2::{Data, Error as JsonRpcError, Id, Params};
use log::debug;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::rpc_util::get_error_obj;
use beacon::Beacon;
use blocks::{
    header::json::BlockHeaderJson, tipset_json::TipsetJson, tipset_keys_json::TipsetKeysJson,
    BlockHeader, Tipset,
};
use blockstore::BlockStore;
use chain::headchange_json::HeadChangeJson;
use cid::{json::CidJson, Cid};
use crypto::DomainSeparationTag;
use message::{
    unsigned_message::{self, json::UnsignedMessageJson},
    UnsignedMessage,
};
use num_traits::FromPrimitive;
use rpc_api::{
    chain_api::*,
    data_types::{BlockMessages, RPCState},
};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Message {
    #[serde(with = "cid::json")]
    cid: Cid,
    #[serde(with = "unsigned_message::json")]
    message: UnsignedMessage,
}

pub(crate) async fn chain_get_message<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainGetMessageParams>,
) -> Result<ChainGetMessageResult, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (CidJson(msg_cid),) = params;
    let ret: UnsignedMessage = data
        .state_manager
        .blockstore()
        .get(&msg_cid)?
        .ok_or("can't find message with that cid")?;
    Ok(UnsignedMessageJson(ret))
}

pub(crate) async fn chain_read_obj<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainReadObjParams>,
) -> Result<ChainReadObjResult, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (CidJson(obj_cid),) = params;
    let ret = data
        .state_manager
        .blockstore()
        .get_bytes(&obj_cid)?
        .ok_or("can't find object with that cid")?;
    Ok(hex::encode(ret))
}

pub(crate) async fn chain_has_obj<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainHasObjParams>,
) -> Result<ChainHasObjResult, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (CidJson(obj_cid),) = params;
    Ok(data
        .state_manager
        .blockstore()
        .get_bytes(&obj_cid)?
        .is_some())
}

pub(crate) async fn chain_get_block_messages<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainGetBlockMessagesParams>,
) -> Result<ChainGetBlockMessagesResult, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (CidJson(blk_cid),) = params;
    let blk: BlockHeader = data
        .state_manager
        .blockstore()
        .get(&blk_cid)?
        .ok_or("can't find block with that cid")?;
    let blk_msgs = blk.messages();
    let (unsigned_cids, signed_cids) =
        chain::read_msg_cids(data.state_manager.blockstore(), &blk_msgs)?;
    let (bls_msg, secp_msg) = chain::block_messages_from_cids(
        data.state_manager.blockstore(),
        &unsigned_cids,
        &signed_cids,
    )?;
    let cids = unsigned_cids
        .into_iter()
        .chain(signed_cids)
        .collect::<Vec<_>>();

    let ret = BlockMessages {
        bls_msg,
        secp_msg,
        cids,
    };
    Ok(ret)
}

pub(crate) async fn chain_get_tipset_by_height<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainGetTipsetByHeightParams>,
) -> Result<ChainGetTipsetByHeightResult, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (height, tsk) = params;
    let ts = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&tsk)
        .await?;
    let tss = data
        .state_manager
        .chain_store()
        .tipset_by_height(height, ts, true)
        .await?;
    Ok(TipsetJson(tss))
}

pub(crate) async fn chain_get_genesis<DB, B>(
    data: Data<RPCState<DB, B>>,
) -> Result<ChainGetGenesisResult, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let genesis =
        chain::genesis(data.state_manager.blockstore())?.ok_or("can't find genesis tipset")?;
    let gen_ts = Arc::new(Tipset::new(vec![genesis])?);
    Ok(Some(TipsetJson(gen_ts)))
}

pub(crate) async fn chain_head<DB, B>(
    data: Data<RPCState<DB, B>>,
) -> Result<ChainHeadResult, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let heaviest = data
        .state_manager
        .chain_store()
        .heaviest_tipset()
        .await
        .ok_or("can't find heaviest tipset")?;
    Ok(TipsetJson(heaviest))
}

pub(crate) async fn chain_head_subscription<DB, B>(
    data: Data<RPCState<DB, B>>,
) -> Result<ChainHeadSubscriptionResult, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let subscription_id = data.state_manager.chain_store().sub_head_changes().await;
    Ok(subscription_id)
}

pub(crate) async fn chain_notify<'a, DB, B>(
    data: Data<RPCState<DB, B>>,
    id: Id,
) -> Result<ChainNotifyResult, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    if let Id::Num(id) = id {
        debug!("Requested ChainNotify from id: {}", id);

        let event = data
            .state_manager
            .chain_store()
            .next_head_change(&id)
            .await
            .unwrap();

        debug!("Responding to ChainNotify from id: {}", id);

        Ok((id, vec![HeadChangeJson::from(event)]))
    } else {
        Err(get_error_obj(-32600, "Invalid request".to_owned()))
    }
}

pub(crate) async fn chain_tipset_weight<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainTipSetWeightParams>,
) -> Result<ChainTipSetWeightResult, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (tsk,) = params;
    let ts = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&tsk.into())
        .await?;
    Ok(ts.weight().to_str_radix(10))
}

pub(crate) async fn chain_get_block<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainGetBlockParams>,
) -> Result<ChainGetBlockResult, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (CidJson(blk_cid),) = params;
    let blk: BlockHeader = data
        .state_manager
        .blockstore()
        .get(&blk_cid)?
        .ok_or("can't find BlockHeader with that cid")?;
    Ok(BlockHeaderJson(blk))
}

pub(crate) async fn chain_get_tipset<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainGetTipSetParams>,
) -> Result<ChainGetTipSetResult, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (TipsetKeysJson(tsk),) = params;
    let ts = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&tsk)
        .await?;
    Ok(TipsetJson(ts))
}

pub(crate) async fn chain_get_randomness_from_tickets<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainGetRandomnessFromTicketsParams>,
) -> Result<ChainGetRandomnessFromTicketsResult, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (TipsetKeysJson(tsk), pers, epoch, entropy) = params;
    let entropy = entropy.unwrap_or_default();
    Ok(data
        .state_manager
        .chain_store()
        .get_chain_randomness(
            &tsk,
            DomainSeparationTag::from_i64(pers).ok_or("invalid DomainSeparationTag")?,
            epoch,
            &base64::decode(entropy)?,
            epoch <= networks::UPGRADE_HYPERDRIVE_HEIGHT,
        )
        .await?)
}

pub(crate) async fn chain_get_randomness_from_beacon<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainGetRandomnessFromBeaconParams>,
) -> Result<ChainGetRandomnessFromBeaconResult, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (TipsetKeysJson(tsk), pers, epoch, entropy) = params;
    let entropy = entropy.unwrap_or_default();

    Ok(data
        .state_manager
        .chain_store()
        .get_beacon_randomness(
            &tsk,
            DomainSeparationTag::from_i64(pers).ok_or("invalid DomainSeparationTag")?,
            epoch,
            &base64::decode(entropy)?,
            epoch <= networks::UPGRADE_HYPERDRIVE_HEIGHT,
        )
        .await?)
}
