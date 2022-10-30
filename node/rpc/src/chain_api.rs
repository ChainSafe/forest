// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::Result;
use cid::Cid;
use forest_beacon::Beacon;
use forest_blocks::{
    header::json::BlockHeaderJson, tipset_json::TipsetJson, tipset_keys_json::TipsetKeysJson,
    BlockHeader, Tipset,
};
use forest_db::Store;
use forest_json::cid::CidJson;
use forest_json::message;
use forest_json::message::json::MessageJson;
use forest_networks::Height;
use forest_rpc_api::{
    chain_api::*,
    data_types::{BlockMessages, RPCState},
};
use forest_utils::db::BlockstoreExt;
use forest_utils::io::AsyncWriterWithChecksum;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::message::Message as FVMMessage;
use hex::ToHex;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use log::{debug, error};
use serde::{Deserialize, Serialize};
use sha2::{digest::Output, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::io::AsyncWriteExt;
use tokio::{fs::File, io::BufWriter};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Message {
    #[serde(with = "forest_json::cid")]
    cid: Cid,
    #[serde(with = "message::json")]
    message: FVMMessage,
}

pub(crate) async fn chain_get_message<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainGetMessageParams>,
) -> Result<ChainGetMessageResult, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (CidJson(msg_cid),) = params;
    let ret: FVMMessage = data
        .state_manager
        .blockstore()
        .get_obj(&msg_cid)?
        .ok_or("can't find message with that cid")?;
    Ok(MessageJson(ret))
}

pub(crate) async fn chain_export<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainExportParams>,
) -> Result<ChainExportResult, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (epoch, recent_roots, include_olds_msgs, out, TipsetKeysJson(tsk), skip_checksum) = params;
    let skip_old_msgs = !include_olds_msgs;

    let chain_finality = data.state_manager.chain_config().policy.chain_finality;
    if recent_roots < chain_finality {
        Err(&format!(
            "recent-stateroots must be greater than {}",
            chain_finality
        ))?;
    }

    let out_tmp = out.with_extension("car.tmp");
    let file = File::create(&out_tmp).await.map_err(JsonRpcError::from)?;
    let writer = AsyncWriterWithChecksum::<Sha256, _>::new(BufWriter::new(file));

    let head = data.chain_store.tipset_from_keys(&tsk).await?;

    let start_ts = data.chain_store.tipset_by_height(epoch, head, true).await?;

    match data
        .chain_store
        .export(&start_ts, recent_roots, skip_old_msgs, writer)
        .await
    {
        Ok(checksum) => {
            std::fs::rename(&out_tmp, &out)?;
            if !skip_checksum {
                save_checksum(&out, checksum).await?;
            }
        }
        Err(e) => {
            if let Err(e) = std::fs::remove_file(&out_tmp) {
                error!(
                    "failed to remove incomplete export file at {}: {e}",
                    out_tmp.display()
                );
            } else {
                debug!("incomplete export file at {} removed", out_tmp.display());
            }

            return Err(JsonRpcError::from(e));
        }
    };

    Ok(out)
}

/// Prints hex-encoded representation of SHA-256 checksum and saves it to a file with the same
/// name but with a `.sha256sum` extension.
async fn save_checksum(source: &Path, hash: Output<Sha256>) -> Result<()> {
    let encoded_hash = hash.encode_hex::<String>();

    let mut checksum_path = PathBuf::from(source);
    checksum_path.set_extension("sha256sum");

    let mut checksum_file = File::create(&checksum_path).await?;
    checksum_file.write_all(encoded_hash.as_bytes()).await?;
    checksum_file.flush().await?;
    log::info!(
        "Snapshot checksum: {encoded_hash} saved to {}",
        checksum_path.display()
    );

    Ok(())
}

pub(crate) async fn chain_read_obj<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainReadObjParams>,
) -> Result<ChainReadObjResult, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (CidJson(obj_cid),) = params;
    let ret = data
        .state_manager
        .blockstore()
        .get(&obj_cid)?
        .ok_or("can't find object with that cid")?;
    Ok(hex::encode(ret))
}

pub(crate) async fn chain_has_obj<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainHasObjParams>,
) -> Result<ChainHasObjResult, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (CidJson(obj_cid),) = params;
    Ok(data.state_manager.blockstore().get(&obj_cid)?.is_some())
}

pub(crate) async fn chain_get_block_messages<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainGetBlockMessagesParams>,
) -> Result<ChainGetBlockMessagesResult, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (CidJson(blk_cid),) = params;
    let blk: BlockHeader = data
        .state_manager
        .blockstore()
        .get_obj(&blk_cid)?
        .ok_or("can't find block with that cid")?;
    let blk_msgs = blk.messages();
    let (unsigned_cids, signed_cids) =
        forest_chain::read_msg_cids(data.state_manager.blockstore(), blk_msgs)?;
    let (bls_msg, secp_msg) = forest_chain::block_messages_from_cids(
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
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
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
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let genesis = forest_chain::genesis(data.state_manager.blockstore())?
        .ok_or("can't find genesis tipset")?;
    let gen_ts = Arc::new(Tipset::new(vec![genesis])?);
    Ok(Some(TipsetJson(gen_ts)))
}

pub(crate) async fn chain_head<DB, B>(
    data: Data<RPCState<DB, B>>,
) -> Result<ChainHeadResult, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let heaviest = data
        .state_manager
        .chain_store()
        .heaviest_tipset()
        .await
        .ok_or("can't find heaviest tipset")?;
    Ok(TipsetJson(heaviest))
}

// XXX: Disable 'chain_head_subscription' because it is unused.
// pub(crate) async fn chain_head_subscription<DB, B>(
//     data: Data<RPCState<DB, B>>,
// ) -> Result<ChainHeadSubscriptionResult, JsonRpcError>
// where
//     DB: Blockstore + Store + Clone + Send + Sync + 'static,
//     B: Beacon + Send + Sync + 'static,
// {
//     let subscription_id = data.state_manager.chain_store().sub_head_changes().await;
//     Ok(subscription_id)
// }

// XXX: Disable 'chain_notify' because it is unused.
// pub(crate) async fn chain_notify<DB, B>(
//     data: Data<RPCState<DB, B>>,
//     id: Id,
// ) -> Result<ChainNotifyResult, JsonRpcError>
// where
//     DB: Blockstore + Store + Clone + Send + Sync + 'static,
//     B: Beacon + Send + Sync + 'static,
// {
//     if let Id::Num(id) = id {
//         debug!("Requested ChainNotify from id: {}", id);

//         let event = data
//             .state_manager
//             .chain_store()
//             .next_head_change(&id)
//             .await
//             .unwrap();

//         debug!("Responding to ChainNotify from id: {}", id);

//         Ok((id, vec![HeadChangeJson::from(event)]))
//     } else {
//         Err(get_error_obj(-32600, "Invalid request".to_owned()))
//     }
// }

pub(crate) async fn chain_tipset_weight<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainTipSetWeightParams>,
) -> Result<ChainTipSetWeightResult, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
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
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (CidJson(blk_cid),) = params;
    let blk: BlockHeader = data
        .state_manager
        .blockstore()
        .get_obj(&blk_cid)?
        .ok_or("can't find BlockHeader with that cid")?;
    Ok(BlockHeaderJson(blk))
}

pub(crate) async fn chain_get_tipset<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainGetTipSetParams>,
) -> Result<ChainGetTipSetResult, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
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
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (TipsetKeysJson(tsk), pers, epoch, entropy) = params;
    let entropy = entropy.unwrap_or_default();
    let hyperdrive_height = data.state_manager.chain_config().epoch(Height::Hyperdrive);
    Ok(data
        .state_manager
        .get_chain_randomness(
            &tsk,
            pers,
            epoch,
            &base64::decode(entropy)?,
            epoch <= hyperdrive_height,
        )
        .await?)
}

pub(crate) async fn chain_get_randomness_from_beacon<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainGetRandomnessFromBeaconParams>,
) -> Result<ChainGetRandomnessFromBeaconResult, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (TipsetKeysJson(tsk), pers, epoch, entropy) = params;
    let entropy = entropy.unwrap_or_default();

    Ok(data
        .state_manager
        .get_beacon_randomness(&tsk, pers, epoch, &base64::decode(entropy)?)
        .await?)
}

pub(crate) async fn chain_get_name<DB, B>(
    data: Data<RPCState<DB, B>>,
) -> Result<ChainGetNameResult, JsonRpcError>
where
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let name: String = data.state_manager.chain_config().name.clone();
    Ok(name)
}
