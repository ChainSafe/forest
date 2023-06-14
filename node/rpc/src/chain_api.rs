// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result};
use forest_beacon::Beacon;
use forest_blocks::{
    header::json::BlockHeaderJson, tipset_json::TipsetJson, tipset_keys_json::TipsetKeysJson,
    BlockHeader, Tipset,
};
use forest_json::{cid::CidJson, message::json::MessageJson};
use forest_rpc_api::{
    chain_api::*,
    data_types::{BlockMessages, RPCState},
};
use forest_shim::message::Message;
use forest_utils::io::VoidAsyncWriter;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::CborStore;
use hex::ToHex;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use sha2::{digest::Output, Sha256};
use tempfile::NamedTempFile;
use tokio::{io::AsyncWriteExt, sync::Mutex};
use tokio_util::compat::TokioAsyncReadCompatExt;

pub(crate) async fn chain_get_message<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainGetMessageParams>,
) -> Result<ChainGetMessageResult, JsonRpcError>
where
    DB: Blockstore + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (CidJson(msg_cid),) = params;
    let ret: Message = data
        .state_manager
        .blockstore()
        .get_cbor(&msg_cid)?
        .ok_or("can't find message with that cid")?;
    Ok(MessageJson(ret))
}

pub(crate) async fn chain_export<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(ChainExportParams {
        epoch,
        recent_roots,
        output_path,
        tipset_keys: TipsetKeysJson(tsk),
        compressed,
        skip_checksum,
        dry_run,
    }): Params<ChainExportParams>,
) -> Result<ChainExportResult, JsonRpcError>
where
    DB: Blockstore + Clone + Send + Sync + 'static,
    B: Beacon,
{
    lazy_static::lazy_static! {
        static ref LOCK: Mutex<()> = Mutex::new(());
    }

    let _locked = LOCK.try_lock();
    if _locked.is_err() {
        return Err(JsonRpcError::Provided {
            code: http::StatusCode::SERVICE_UNAVAILABLE.as_u16() as _,
            message: "Another chain export job is still in progress",
        });
    }

    let chain_finality = data.state_manager.chain_config().policy.chain_finality;
    if recent_roots < chain_finality {
        Err(&format!(
            "recent-stateroots must be greater than {chain_finality}"
        ))?;
    }

    let output_dir = output_path.parent().ok_or_else(|| JsonRpcError::Provided {
        code: http::StatusCode::INTERNAL_SERVER_ERROR.as_u16() as _,
        message: "Failed to determine snapshot export directory",
    })?;
    let temp_path = NamedTempFile::new_in(output_dir)?.into_temp_path();
    let head = data.chain_store.tipset_from_keys(&tsk)?;
    let start_ts = data.chain_store.tipset_by_height(epoch, head, true)?;

    match if dry_run {
        data.chain_store
            .export::<_, Sha256>(
                &start_ts,
                recent_roots,
                VoidAsyncWriter::default(),
                compressed,
                skip_checksum,
            )
            .await
    } else {
        let file = tokio::fs::File::create(&temp_path).await?;
        data.chain_store
            .export::<_, Sha256>(
                &start_ts,
                recent_roots,
                file.compat(),
                compressed,
                skip_checksum,
            )
            .await
    } {
        Ok(checksum_opt) if !dry_run => {
            // `persist`is expected to succeed since we've made sure the temp-file is in the
            // same folder as the final file.
            temp_path.persist(&output_path)?;
            if let Some(checksum) = checksum_opt {
                save_checksum(&output_path, checksum).await?;
            }
        }
        Ok(_) => {}
        Err(e) => {
            return Err(JsonRpcError::from(e));
        }
    };

    Ok(output_path)
}

/// Prints hex-encoded representation of SHA-256 checksum and saves it to a file
/// with the same name but with a `.sha256sum` extension.
async fn save_checksum(source: &Path, hash: Output<Sha256>) -> Result<()> {
    let encoded_hash = hash.encode_hex::<String>();
    let checksum_file_content = format!(
        "{encoded_hash} {}\n",
        source
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .context("Failed to retrieve file name while saving checksum")?
    );

    let mut checksum_path = PathBuf::from(source);
    checksum_path.set_extension("sha256sum");

    let mut checksum_file = tokio::fs::File::create(&checksum_path).await?;
    checksum_file
        .write_all(checksum_file_content.as_bytes())
        .await?;
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
    DB: Blockstore + Clone + Send + Sync + 'static,
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
    DB: Blockstore + Clone + Send + Sync + 'static,
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
    DB: Blockstore + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (CidJson(blk_cid),) = params;
    let blk: BlockHeader = data
        .state_manager
        .blockstore()
        .get_cbor(&blk_cid)?
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
    DB: Blockstore + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (height, tsk) = params;
    let ts = data.state_manager.chain_store().tipset_from_keys(&tsk)?;
    let tss = data
        .state_manager
        .chain_store()
        .tipset_by_height(height, ts, true)?;
    Ok(TipsetJson(tss))
}

pub(crate) async fn chain_get_genesis<DB, B>(
    data: Data<RPCState<DB, B>>,
) -> Result<ChainGetGenesisResult, JsonRpcError>
where
    DB: Blockstore + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let genesis = data.state_manager.chain_store().genesis()?;
    let gen_ts = Arc::new(Tipset::from(genesis));
    Ok(Some(TipsetJson(gen_ts)))
}

pub(crate) async fn chain_head<DB, B>(
    data: Data<RPCState<DB, B>>,
) -> Result<ChainHeadResult, JsonRpcError>
where
    DB: Blockstore + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let heaviest = data.state_manager.chain_store().heaviest_tipset();
    Ok(TipsetJson(heaviest))
}

pub(crate) async fn chain_get_block<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainGetBlockParams>,
) -> Result<ChainGetBlockResult, JsonRpcError>
where
    DB: Blockstore + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (CidJson(blk_cid),) = params;
    let blk: BlockHeader = data
        .state_manager
        .blockstore()
        .get_cbor(&blk_cid)?
        .ok_or("can't find BlockHeader with that cid")?;
    Ok(BlockHeaderJson(blk))
}

pub(crate) async fn chain_get_tipset<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainGetTipSetParams>,
) -> Result<ChainGetTipSetResult, JsonRpcError>
where
    DB: Blockstore + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (TipsetKeysJson(tsk),) = params;
    let ts = data.state_manager.chain_store().tipset_from_keys(&tsk)?;
    Ok(TipsetJson(ts))
}

pub(crate) async fn chain_get_tipset_hash<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainGetTipSetHashParams>,
) -> Result<ChainGetTipSetHashResult, JsonRpcError>
where
    DB: Blockstore + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (TipsetKeysJson(tsk),) = params;
    let ts = data.state_manager.chain_store().tipset_hash_from_keys(&tsk);
    Ok(ts)
}

pub(crate) async fn chain_validate_tipset_checkpoints<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainValidateTipSetCheckpointsParams>,
) -> Result<ChainValidateTipSetCheckpointsResult, JsonRpcError>
where
    DB: Blockstore + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let () = params;

    let tipset = data.state_manager.chain_store().heaviest_tipset();
    let ts = data
        .state_manager
        .chain_store()
        .tipset_from_keys(tipset.key())?;
    data.state_manager
        .chain_store()
        .validate_tipset_checkpoints(ts, &data.state_manager.chain_config().network)?;
    Ok("Ok".to_string())
}

pub(crate) async fn chain_get_name<DB, B>(
    data: Data<RPCState<DB, B>>,
) -> Result<ChainGetNameResult, JsonRpcError>
where
    DB: Blockstore + Clone + Send + Sync + 'static,
    B: Beacon,
{
    Ok(data.state_manager.chain_config().network.to_string())
}

// This is basically a port of the reference implementation at
// https://github.com/filecoin-project/lotus/blob/v1.23.0/node/impl/full/chain.go#L321
pub(crate) async fn chain_set_head<DB, B>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<ChainSetHeadParams>,
) -> Result<ChainSetHeadResult, JsonRpcError>
where
    DB: Blockstore + Clone + Send + Sync + 'static,
    B: Beacon,
{
    let (params,) = params;
    let new_head = data.state_manager.chain_store().tipset_from_keys(&params)?;
    let mut current = data.state_manager.chain_store().heaviest_tipset();
    while current.epoch() >= new_head.epoch() {
        for cid in current.key().cids() {
            data.state_manager
                .chain_store()
                .unmark_block_as_validated(cid);
        }
        let parents = current.blocks()[0].parents();
        current = data.state_manager.chain_store().tipset_from_keys(parents)?;
    }
    data.state_manager
        .chain_store()
        .set_heaviest_tipset(new_head)
        .map_err(Into::into)
}
