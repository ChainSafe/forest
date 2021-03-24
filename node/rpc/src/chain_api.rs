// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use jsonrpc_v2::{Data, Error as JsonRpcError, Id, Params};
use log::debug;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::data_types::SubscriptionHeadChange;
use crate::rpc_util::get_error_obj;
use crate::RpcState;
use beacon::Beacon;
use blocks::{
    header::json::BlockHeaderJson, tipset_json::TipsetJson, tipset_keys_json::TipsetKeysJson,
    BlockHeader, Tipset, TipsetKeys,
};
use blockstore::BlockStore;
use chain::headchange_json::HeadChangeJson;
use cid::{json::CidJson, Cid};
use clock::ChainEpoch;
use crypto::DomainSeparationTag;

use message::{
    signed_message,
    unsigned_message::{self, json::UnsignedMessageJson},
    SignedMessage, UnsignedMessage,
};
use num_traits::FromPrimitive;
use wallet::KeyStore;

#[derive(Serialize, Deserialize)]
pub(crate) struct BlockMessages {
    #[serde(rename = "BlsMessages", with = "unsigned_message::json::vec")]
    pub bls_msg: Vec<UnsignedMessage>,
    #[serde(rename = "SecpkMessages", with = "signed_message::json::vec")]
    pub secp_msg: Vec<SignedMessage>,
    #[serde(rename = "Cids", with = "cid::json::vec")]
    pub cids: Vec<Cid>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Message {
    #[serde(with = "cid::json")]
    cid: Cid,
    #[serde(with = "unsigned_message::json")]
    message: UnsignedMessage,
}

pub(crate) async fn chain_get_message<DB, KS, B>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(CidJson,)>,
) -> Result<UnsignedMessageJson, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
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

pub(crate) async fn chain_read_obj<DB, KS, B>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(CidJson,)>,
) -> Result<String, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (CidJson(obj_cid),) = params;
    let ret = data
        .state_manager
        .blockstore()
        .get_bytes(&obj_cid)?
        .ok_or("can't find object with that cid")?;
    Ok(base64::encode(ret))
}

pub(crate) async fn chain_has_obj<DB, KS, B>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(CidJson,)>,
) -> Result<bool, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let (CidJson(obj_cid),) = params;
    Ok(data
        .state_manager
        .blockstore()
        .get_bytes(&obj_cid)?
        .is_some())
}

pub(crate) async fn chain_block_messages<DB, KS, B>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(CidJson,)>,
) -> Result<BlockMessages, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
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

pub(crate) async fn chain_get_tipset_by_height<DB, KS, B>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(ChainEpoch, TipsetKeys)>,
) -> Result<TipsetJson, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
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

pub(crate) async fn chain_get_genesis<DB, KS, B>(
    data: Data<RpcState<DB, KS, B>>,
) -> Result<Option<TipsetJson>, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let genesis =
        chain::genesis(data.state_manager.blockstore())?.ok_or("can't find genesis tipset")?;
    let gen_ts = Arc::new(Tipset::new(vec![genesis])?);
    Ok(Some(TipsetJson(gen_ts)))
}

pub(crate) async fn chain_head<DB, KS, B>(
    data: Data<RpcState<DB, KS, B>>,
) -> Result<TipsetJson, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
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

pub(crate) async fn chain_head_sub<DB, KS, B>(
    data: Data<RpcState<DB, KS, B>>,
) -> Result<i64, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    let subscription_id = data.state_manager.chain_store().sub_head_changes().await;
    Ok(subscription_id)
}

pub(crate) async fn chain_notify<'a, DB, KS, B>(
    data: Data<RpcState<DB, KS, B>>,
    id: Id,
) -> Result<SubscriptionHeadChange, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
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

pub(crate) async fn chain_tipset_weight<DB, KS, B>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(TipsetKeysJson,)>,
) -> Result<String, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
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

pub(crate) async fn chain_get_block<DB, KS, B>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(CidJson,)>,
) -> Result<BlockHeaderJson, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
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

pub(crate) async fn chain_get_tipset<DB, KS, B>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(TipsetKeysJson,)>,
) -> Result<TipsetJson, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
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

pub(crate) async fn chain_get_randomness_from_tickets<DB, KS, B>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(TipsetKeysJson, i64, ChainEpoch, Option<String>)>,
) -> Result<[u8; 32], JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
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
        )
        .await?)
}

pub(crate) async fn chain_get_randomness_from_beacon<DB, KS, B>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(TipsetKeysJson, i64, ChainEpoch, Option<String>)>,
) -> Result<[u8; 32], JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
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
        )
        .await?)
}
