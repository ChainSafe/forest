// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod types;
use types::*;

#[cfg(test)]
use crate::blocks::RawBlockHeader;
use crate::blocks::{Block, CachingBlockHeader, Tipset, TipsetKey};
use crate::chain::index::ResolveNullTipset;
use crate::chain::{ChainStore, ExportOptions, FilecoinSnapshotVersion, HeadChange};
use crate::chain_sync::{get_full_tipset, load_full_tipset};
use crate::cid_collections::{CidHashSet, FileBackedCidHashSet};
use crate::ipld::DfsIter;
use crate::ipld::{CHAIN_EXPORT_STATUS, cancel_export, end_export, start_export};
use crate::lotus_json::{HasLotusJson, LotusJson, lotus_json_with_self};
#[cfg(test)]
use crate::lotus_json::{assert_all_snapshots, assert_unchanged_via_json};
use crate::message::{ChainMessage, SignedMessage};
use crate::prelude::*;
use crate::rpc::eth::Block as EthBlock;
use crate::rpc::eth::{
    EthLog, TxInfo, eth_logs_with_filter, types::ApiHeaders, types::EthFilterSpec,
};
use crate::rpc::f3::F3ExportLatestSnapshot;
use crate::rpc::types::*;
use crate::rpc::{ApiPaths, Ctx, EthEventHandler, Permission, RpcMethod, ServerError};
use crate::shim::clock::ChainEpoch;
use crate::shim::error::ExitCode;
use crate::shim::executor::Receipt;
use crate::shim::message::Message;
use crate::utils::db::CborStoreExt as _;
use crate::utils::io::VoidAsyncWriter;
use crate::utils::misc::env::is_env_truthy;
use anyhow::{Context as _, Result};
use enumflags2::{BitFlags, make_bitflags};
use fvm_ipld_encoding::{CborStore, RawBytes};
use hex::ToHex;
use ipld_core::ipld::Ipld;
use jsonrpsee::types::Params;
use jsonrpsee::types::error::ErrorObjectOwned;
use num::BigInt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::fs::File;
use std::{collections::VecDeque, path::PathBuf, sync::LazyLock};
use tokio::sync::{
    Mutex,
    broadcast::{self, Receiver as Subscriber},
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

const HEAD_CHANNEL_CAPACITY: usize = 10;

/// [`SAFE_HEIGHT_DISTANCE`] is the distance from the latest tipset, i.e. "heaviest", that
/// is considered to be safe from re-orgs at an increasingly diminishing
/// probability.
///
/// This is used to determine the safe tipset when using the "safe" tag in
/// [`TipsetSelector`] or via Eth JSON-RPC APIs. Note that "safe" doesn't guarantee
/// finality, but rather a high probability of not being reverted. For guaranteed
/// finality, use the "finalized" tag.
///
/// This constant is experimental and may change in the future.
/// Discussion on this current value and a tracking item to document the
/// probabilistic impact of various values is in
/// https://github.com/filecoin-project/go-f3/issues/944
pub const SAFE_HEIGHT_DISTANCE: ChainEpoch = 200;

static CHAIN_EXPORT_LOCK: LazyLock<Mutex<Option<CancellationToken>>> =
    LazyLock::new(|| Mutex::new(None));

/// Subscribes to head changes from the chain store and broadcasts new blocks.
///
/// # Notes
///
/// Spawns an internal `tokio` task that can be aborted anytime via the returned `JoinHandle`,
/// allowing manual cleanup if needed.
pub(crate) fn new_heads(data: Ctx) -> (Subscriber<ApiHeaders>, JoinHandle<()>) {
    let (sender, receiver) = broadcast::channel(HEAD_CHANNEL_CAPACITY);

    let mut head_changes_rx = data.chain_store().subscribe_head_changes();

    let handle = tokio::spawn(async move {
        while let Ok(changes) = head_changes_rx.recv().await {
            for ts in changes.applies {
                // Convert the tipset to an Ethereum block with full transaction info
                // Note: In Filecoin's Eth RPC, a tipset maps to a single Ethereum block
                match EthBlock::from_filecoin_tipset(data.clone(), ts, TxInfo::Full).await {
                    Ok(block) => {
                        if let Err(e) = sender.send(ApiHeaders(block)) {
                            tracing::error!("Failed to send headers: {}", e);
                            return;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to convert tipset to eth block: {}", e);
                    }
                }
            }
        }
    });

    (receiver, handle)
}

/// Subscribes to head changes from the chain store and broadcasts new `Ethereum` logs.
///
/// # Notes
///
/// Spawns an internal `tokio` task that can be aborted anytime via the returned `JoinHandle`,
/// allowing manual cleanup if needed.
pub(crate) fn logs(
    ctx: &Ctx,
    filter: Option<EthFilterSpec>,
) -> (Subscriber<Vec<EthLog>>, JoinHandle<()>) {
    let (sender, receiver) = broadcast::channel(HEAD_CHANNEL_CAPACITY);

    let mut head_changes_rx = ctx.chain_store().subscribe_head_changes();

    let ctx = ctx.clone();

    let handle = tokio::spawn(async move {
        while let Ok(changes) = head_changes_rx.recv().await {
            for ts in changes.applies {
                match eth_logs_with_filter(&ctx, &ts, filter.clone()).await {
                    Ok(logs) => {
                        if !logs.is_empty()
                            && let Err(e) = sender.send(logs)
                        {
                            tracing::error!("Failed to send logs for tipset {}: {}", ts.key(), e);
                            break;
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to fetch logs for tipset {}: {}", ts.key(), e);
                    }
                }
            }
        }
    });

    (receiver, handle)
}

pub enum ChainGetFinalizedTipset {}
impl RpcMethod<0> for ChainGetFinalizedTipset {
    const NAME: &'static str = "Filecoin.ChainGetFinalizedTipSet";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::V1);
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Returns the latest F3 finalized tipset, or falls back to EC finality if F3 is not operational on the node or if the F3 finalized tipset is further back than EC finalized tipset.",
    );

    type Params = ();
    type Ok = Tipset;

    async fn handle(
        ctx: Ctx,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(ChainGetTipSetV2::get_latest_finalized_tipset(&ctx).await?)
    }
}

pub enum ChainGetMessage {}
impl RpcMethod<1> for ChainGetMessage {
    const NAME: &'static str = "Filecoin.ChainGetMessage";
    const PARAM_NAMES: [&'static str; 1] = ["messageCid"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns the message with the specified CID.");

    type Params = (Cid,);
    type Ok = Message;

    async fn handle(
        ctx: Ctx,
        (message_cid,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let chain_message: ChainMessage = ctx
            .db()
            .get_cbor(&message_cid)?
            .with_context(|| format!("can't find message with cid {message_cid}"))?;
        let message = match chain_message {
            ChainMessage::Signed(m) => Arc::unwrap_or_clone(m).into_message(),
            ChainMessage::Unsigned(m) => Arc::unwrap_or_clone(m),
        };

        Ok(message)
    }
}

/// Returns the events stored under the given event AMT root CID.
/// Errors if the root CID cannot be found in the blockstore.
pub enum ChainGetEvents {}
impl RpcMethod<1> for ChainGetEvents {
    const NAME: &'static str = "Filecoin.ChainGetEvents";
    const PARAM_NAMES: [&'static str; 1] = ["rootCid"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the events under the given event AMT root CID.");

    type Params = (Cid,);
    type Ok = Vec<Event>;
    async fn handle(
        ctx: Ctx,
        (root_cid,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let events = EthEventHandler::get_events_by_event_root(&ctx, &root_cid)?;
        Ok(events)
    }
}

pub enum ChainGetParentMessages {}
impl RpcMethod<1> for ChainGetParentMessages {
    const NAME: &'static str = "Filecoin.ChainGetParentMessages";
    const PARAM_NAMES: [&'static str; 1] = ["blockCid"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the messages included in the blocks of the parent tipset.");

    type Params = (Cid,);
    type Ok = Vec<ApiMessage>;

    async fn handle(
        ctx: Ctx,
        (block_cid,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let store = ctx.db();
        let block_header: CachingBlockHeader = store
            .get_cbor(&block_cid)?
            .with_context(|| format!("can't find block header with cid {block_cid}"))?;
        if block_header.epoch == 0 {
            Ok(vec![])
        } else {
            let parent_tipset = ctx
                .chain_index()
                .load_required_tipset(&block_header.parents)?;
            load_api_messages_from_tipset(&ctx, parent_tipset.key()).await
        }
    }
}

pub enum ChainGetParentReceipts {}
impl RpcMethod<1> for ChainGetParentReceipts {
    const NAME: &'static str = "Filecoin.ChainGetParentReceipts";
    const PARAM_NAMES: [&'static str; 1] = ["blockCid"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the message receipts included in the blocks of the parent tipset.");

    type Params = (Cid,);
    type Ok = Vec<ApiReceipt>;

    async fn handle(
        ctx: Ctx,
        (block_cid,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let store = ctx.db();
        let block_header: CachingBlockHeader = store
            .get_cbor(&block_cid)?
            .with_context(|| format!("can't find block header with cid {block_cid}"))?;
        if block_header.epoch == 0 {
            return Ok(vec![]);
        }
        let receipts = Receipt::get_receipts(store, block_header.message_receipts)
            .map_err(|_| {
                ErrorObjectOwned::owned::<()>(
                    1,
                    format!(
                        "failed to root: ipld: could not find {}",
                        block_header.message_receipts
                    ),
                    None,
                )
            })?
            .iter()
            .map(|r| ApiReceipt {
                exit_code: r.exit_code().into(),
                return_data: r.return_data(),
                gas_used: r.gas_used(),
                events_root: r.events_root(),
            })
            .collect_vec();

        Ok(receipts)
    }
}

pub enum ChainGetMessagesInTipset {}
impl RpcMethod<1> for ChainGetMessagesInTipset {
    const NAME: &'static str = "Filecoin.ChainGetMessagesInTipset";
    const PARAM_NAMES: [&'static str; 1] = ["tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (ApiTipsetKey,);
    type Ok = Vec<ApiMessage>;

    async fn handle(
        ctx: Ctx,
        (ApiTipsetKey(tipset_key),): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let tipset = ctx
            .chain_store()
            .load_required_tipset_or_heaviest(&tipset_key)?;
        load_api_messages_from_tipset(&ctx, tipset.key()).await
    }
}

pub enum ChainPruneSnapshot {}
impl RpcMethod<1> for ChainPruneSnapshot {
    const NAME: &'static str = "Forest.SnapshotGC";
    const PARAM_NAMES: [&'static str; 1] = ["blocking"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Admin;

    type Params = (bool,);
    type Ok = ();

    async fn handle(
        _ctx: Ctx,
        (blocking,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        if let Some(gc) = crate::daemon::GLOBAL_SNAPSHOT_GC.get() {
            let progress_rx = gc.trigger()?;
            while blocking && progress_rx.recv_async().await.is_ok() {}
            Ok(())
        } else {
            Err(anyhow::anyhow!("snapshot gc is not enabled").into())
        }
    }
}

pub enum ForestChainExport {}
impl RpcMethod<1> for ForestChainExport {
    const NAME: &'static str = "Forest.ChainExport";
    const PARAM_NAMES: [&'static str; 1] = ["params"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = (ForestChainExportParams,);
    type Ok = ApiExportResult;

    async fn handle(
        ctx: Ctx,
        (params,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ForestChainExportParams {
            version,
            epoch,
            recent_roots,
            output_path,
            tipset_keys: ApiTipsetKey(tsk),
            include_receipts,
            include_events,
            include_tipset_keys,
            skip_checksum,
            dry_run,
        } = params;

        let token = CancellationToken::new();
        {
            let mut guard = CHAIN_EXPORT_LOCK.lock().await;
            if guard.is_some() {
                return Err(
                    anyhow::anyhow!("A chain export is still in progress. Cancel it with the export-cancel subcommand if needed.").into(),
                );
            }
            *guard = Some(token.clone());
        }
        start_export();

        let head = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let start_ts = ctx.chain_index().load_required_tipset_by_height(
            epoch,
            head,
            ResolveNullTipset::TakeOlder,
        )?;

        let options = ExportOptions {
            skip_checksum,
            include_receipts,
            include_events,
            include_tipset_keys,
            seen: FileBackedCidHashSet::new(ctx.temp_dir.as_path())?,
        };
        let writer = if dry_run {
            tokio_util::either::Either::Left(VoidAsyncWriter)
        } else {
            tokio_util::either::Either::Right(tokio::fs::File::create(&output_path).await?)
        };
        let result = match version {
            FilecoinSnapshotVersion::V1 => {
                let db = ctx.db_owned();

                let chain_export = crate::chain::export::<Sha256, _>(
                    &db,
                    &start_ts,
                    recent_roots,
                    writer,
                    options,
                );

                tokio::select! {
                    result = chain_export => {
                        result.map(|checksum_opt| ApiExportResult::Done(checksum_opt.map(|hash| hash.encode_hex())))
                    },
                    _ = token.cancelled() => {
                        cancel_export();
                        tracing::warn!("Snapshot export was cancelled");
                        Ok(ApiExportResult::Cancelled)
                    },
                }
            }
            FilecoinSnapshotVersion::V2 => {
                let db = ctx.db_owned();

                let f3_snap_tmp_path = {
                    let mut f3_snap_dir = output_path.clone();
                    let mut builder = tempfile::Builder::new();
                    let with_suffix = builder.suffix(".f3snap.bin");
                    if f3_snap_dir.pop() {
                        with_suffix.tempfile_in(&f3_snap_dir)
                    } else {
                        with_suffix.tempfile_in(".")
                    }?
                    .into_temp_path()
                };
                let f3_snap = {
                    match F3ExportLatestSnapshot::run(f3_snap_tmp_path.display().to_string()).await
                    {
                        Ok(cid) => Some((cid, File::open(&f3_snap_tmp_path)?)),
                        Err(e) => {
                            tracing::error!("Failed to export F3 snapshot: {e:#}");
                            None
                        }
                    }
                };

                let chain_export = crate::chain::export_v2::<Sha256, _, _>(
                    &db,
                    f3_snap,
                    &start_ts,
                    recent_roots,
                    writer,
                    options,
                );

                tokio::select! {
                    result = chain_export => {
                        result.map(|checksum_opt| ApiExportResult::Done(checksum_opt.map(|hash| hash.encode_hex())))
                    },
                    _ = token.cancelled() => {
                        cancel_export();
                        tracing::warn!("Snapshot export was cancelled");
                        Ok(ApiExportResult::Cancelled)
                    },
                }
            }
        };
        end_export();
        // Clean up token
        let mut guard = CHAIN_EXPORT_LOCK.lock().await;
        *guard = None;
        match result {
            Ok(export_result) => Ok(export_result),
            Err(e) => Err(anyhow::anyhow!(e).into()),
        }
    }
}

pub enum ForestChainExportStatus {}
impl RpcMethod<0> for ForestChainExportStatus {
    const NAME: &'static str = "Forest.ChainExportStatus";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = ApiExportStatus;

    async fn handle(
        _ctx: Ctx,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let mutex = CHAIN_EXPORT_STATUS.lock();

        let progress = if mutex.initial_epoch == 0 {
            0.0
        } else {
            let p = 1.0 - ((mutex.epoch as f64) / (mutex.initial_epoch as f64));
            if p.is_finite() {
                p.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        // only two decimal places
        let progress = (progress * 100.0).round() / 100.0;

        let status = ApiExportStatus {
            progress,
            exporting: mutex.exporting,
            cancelled: mutex.cancelled,
            start_time: mutex.start_time,
        };

        Ok(status)
    }
}

pub enum ForestChainExportCancel {}
impl RpcMethod<0> for ForestChainExportCancel {
    const NAME: &'static str = "Forest.ChainExportCancel";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = bool;

    async fn handle(
        _ctx: Ctx,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        if let Some(token) = CHAIN_EXPORT_LOCK.lock().await.as_ref() {
            token.cancel();
            return Ok(true);
        }

        Ok(false)
    }
}

pub enum ForestChainExportDiff {}
impl RpcMethod<1> for ForestChainExportDiff {
    const NAME: &'static str = "Forest.ChainExportDiff";
    const PARAM_NAMES: [&'static str; 1] = ["params"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all_with_v2();
    const PERMISSION: Permission = Permission::Read;

    type Params = (ForestChainExportDiffParams,);
    type Ok = ();

    async fn handle(
        ctx: Ctx,
        (params,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ForestChainExportDiffParams {
            from,
            to,
            depth,
            output_path,
        } = params;

        let _locked = CHAIN_EXPORT_LOCK.try_lock();
        if _locked.is_err() {
            return Err(
                anyhow::anyhow!("Another chain export diff job is still in progress").into(),
            );
        }

        let chain_finality = ctx.chain_config().policy.chain_finality;
        if depth < chain_finality {
            return Err(
                anyhow::anyhow!(format!("depth must be greater than {chain_finality}")).into(),
            );
        }

        let head = ctx.chain_store().heaviest_tipset();
        let start_ts = ctx.chain_index().load_required_tipset_by_height(
            from,
            head,
            ResolveNullTipset::TakeOlder,
        )?;

        crate::tool::subcommands::archive_cmd::do_export(
            ctx.chain_index().db(),
            start_ts,
            output_path,
            None,
            depth,
            Some(to),
            Some(chain_finality),
            true,
        )
        .await?;

        Ok(())
    }
}

pub enum ChainExport {}
impl RpcMethod<1> for ChainExport {
    const NAME: &'static str = "Filecoin.ChainExport";
    const PARAM_NAMES: [&'static str; 1] = ["params"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (ChainExportParams,);
    type Ok = ApiExportResult;

    async fn handle(
        ctx: Ctx,
        (ChainExportParams {
            epoch,
            recent_roots,
            output_path,
            tipset_keys,
            skip_checksum,
            dry_run,
        },): Self::Params,
        ext: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        ForestChainExport::handle(
            ctx,
            (ForestChainExportParams {
                version: FilecoinSnapshotVersion::V1,
                epoch,
                recent_roots,
                output_path,
                tipset_keys,
                include_receipts: false,
                include_events: false,
                include_tipset_keys: false,
                skip_checksum,
                dry_run,
            },),
            ext,
        )
        .await
    }
}

pub enum ChainReadObj {}
impl RpcMethod<1> for ChainReadObj {
    const NAME: &'static str = "Filecoin.ChainReadObj";
    const PARAM_NAMES: [&'static str; 1] = ["cid"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Reads IPLD nodes referenced by the specified CID from the chain blockstore and returns raw bytes.",
    );

    type Params = (Cid,);
    type Ok = Vec<u8>;

    async fn handle(
        ctx: Ctx,
        (cid,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let bytes = ctx
            .db()
            .get(&cid)?
            .with_context(|| format!("can't find object with cid={cid}"))?;
        Ok(bytes)
    }
}

pub enum ChainHasObj {}
impl RpcMethod<1> for ChainHasObj {
    const NAME: &'static str = "Filecoin.ChainHasObj";
    const PARAM_NAMES: [&'static str; 1] = ["cid"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Checks if a given CID exists in the chain blockstore.");

    type Params = (Cid,);
    type Ok = bool;

    async fn handle(
        ctx: Ctx,
        (cid,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(ctx.db().get(&cid)?.is_some())
    }
}

/// Returns statistics about the graph referenced by 'obj'.
/// If 'base' is also specified, then the returned stat will be a diff between the two objects.
pub enum ChainStatObj {}
impl RpcMethod<2> for ChainStatObj {
    const NAME: &'static str = "Filecoin.ChainStatObj";
    const PARAM_NAMES: [&'static str; 2] = ["obj_cid", "base_cid"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (Cid, Option<Cid>);
    type Ok = ObjStat;

    async fn handle(
        ctx: Ctx,
        (obj_cid, base_cid): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let mut stats = ObjStat::default();
        let mut seen = CidHashSet::default();
        let mut walk = |cid, collect| {
            let mut queue = VecDeque::new();
            queue.push_back(cid);
            while let Some(link_cid) = queue.pop_front() {
                if !seen.insert(link_cid) {
                    continue;
                }
                let data = ctx.db().get(&link_cid)?;
                if let Some(data) = data {
                    if collect {
                        stats.links += 1;
                        stats.size += data.len();
                    }
                    if matches!(link_cid.codec(), fvm_ipld_encoding::DAG_CBOR)
                        && let Ok(ipld) =
                            crate::utils::encoding::from_slice_with_fallback::<Ipld>(&data)
                    {
                        for ipld in DfsIter::new(ipld) {
                            if let Ipld::Link(cid) = ipld {
                                queue.push_back(cid);
                            }
                        }
                    }
                }
            }
            anyhow::Ok(())
        };
        if let Some(base_cid) = base_cid {
            walk(base_cid, false)?;
        }
        walk(obj_cid, true)?;
        Ok(stats)
    }
}

pub enum ChainGetBlockMessages {}
impl RpcMethod<1> for ChainGetBlockMessages {
    const NAME: &'static str = "Filecoin.ChainGetBlockMessages";
    const PARAM_NAMES: [&'static str; 1] = ["blockCid"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns all messages from the specified block.");

    type Params = (Cid,);
    type Ok = BlockMessages;

    async fn handle(
        ctx: Ctx,
        (block_cid,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let blk: CachingBlockHeader = ctx.db().get_cbor_required(&block_cid)?;
        let (unsigned_cids, signed_cids) = crate::chain::read_msg_cids(ctx.db(), &blk)?;
        let (bls_msg, secp_msg) =
            crate::chain::block_messages_from_cids(ctx.db(), &unsigned_cids, &signed_cids)?;
        let cids = unsigned_cids.into_iter().chain(signed_cids).collect();

        let ret = BlockMessages {
            bls_msg,
            secp_msg,
            cids,
        };
        Ok(ret)
    }
}

pub enum ChainGetPath {}
impl RpcMethod<2> for ChainGetPath {
    const NAME: &'static str = "Filecoin.ChainGetPath";
    const PARAM_NAMES: [&'static str; 2] = ["from", "to"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns the path between the two specified tipsets.");

    type Params = (TipsetKey, TipsetKey);
    type Ok = Vec<PathChange>;

    async fn handle(
        ctx: Ctx,
        (from, to): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(chain_get_path(ctx.chain_store(), &from, &to)?.into_change_vec())
    }
}

/// Find the path between two tipsets, as [`PathChanges`].
///
/// ```text
/// 0 - A - B - C - D
///     ^~~~~~~~> apply B, C
///
/// 0 - A - B - C - D
///     <~~~~~~~^ revert C, B
///
///     <~~~~~~~~ revert C, B
/// 0 - A - B  - C
///     |
///      -- B' - C'
///      ~~~~~~~~> then apply B', C'
/// ```
///
/// Exposes errors from the [`Blockstore`], and returns an error if there is no common ancestor.
pub fn chain_get_path(
    chain_store: &ChainStore,
    from: &TipsetKey,
    to: &TipsetKey,
) -> anyhow::Result<PathChanges> {
    let finality = chain_store.chain_config().policy.chain_finality;
    let mut to_revert = chain_store
        .load_required_tipset_or_heaviest(from)
        .context("couldn't load `from`")?;
    let mut to_apply = chain_store
        .load_required_tipset_or_heaviest(to)
        .context("couldn't load `to`")?;

    anyhow::ensure!(
        (to_apply.epoch() - to_revert.epoch()).abs() <= finality,
        "the gap between the new head ({}) and the old head ({}) is larger than chain finality ({finality})",
        to_apply.epoch(),
        to_revert.epoch()
    );

    let mut reverts = vec![];
    let mut applies = vec![];

    // This loop is guaranteed to terminate if the blockstore contain no cycles.
    // This is currently computationally infeasible.
    while to_revert != to_apply {
        if to_revert.epoch() > to_apply.epoch() {
            let next = chain_store
                .load_required_tipset_or_heaviest(to_revert.parents())
                .context("couldn't load ancestor of `from`")?;
            reverts.push(to_revert);
            to_revert = next;
        } else {
            let next = chain_store
                .load_required_tipset_or_heaviest(to_apply.parents())
                .context("couldn't load ancestor of `to`")?;
            applies.push(to_apply);
            to_apply = next;
        }
    }
    applies.reverse();
    Ok(PathChanges { reverts, applies })
}

/// Get tipset at epoch. Pick younger tipset if epoch points to a
/// null-tipset. Only tipsets below the given `head` are searched. If `head`
/// is null, the node will use the heaviest tipset.
pub enum ChainGetTipSetByHeight {}
impl RpcMethod<2> for ChainGetTipSetByHeight {
    const NAME: &'static str = "Filecoin.ChainGetTipSetByHeight";
    const PARAM_NAMES: [&'static str; 2] = ["height", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns the tipset at the specified height.");

    type Params = (ChainEpoch, ApiTipsetKey);
    type Ok = Tipset;

    async fn handle(
        ctx: Ctx,
        (height, ApiTipsetKey(tipset_key)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx
            .chain_store()
            .load_required_tipset_or_heaviest(&tipset_key)?;
        let tss = ctx.chain_index().load_required_tipset_by_height(
            height,
            ts,
            ResolveNullTipset::TakeOlder,
        )?;
        Ok(tss)
    }
}

pub enum ChainGetTipSetAfterHeight {}
impl RpcMethod<2> for ChainGetTipSetAfterHeight {
    const NAME: &'static str = "Filecoin.ChainGetTipSetAfterHeight";
    const PARAM_NAMES: [&'static str; 2] = ["height", "tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some(
        "Looks back and returns the tipset at the specified epoch.
    If there are no blocks at the given epoch,
    returns the first non-nil tipset at a later epoch.",
    );

    type Params = (ChainEpoch, ApiTipsetKey);
    type Ok = Tipset;

    async fn handle(
        ctx: Ctx,
        (height, ApiTipsetKey(tipset_key)): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx
            .chain_store()
            .load_required_tipset_or_heaviest(&tipset_key)?;
        let tss = ctx.chain_index().load_required_tipset_by_height(
            height,
            ts,
            ResolveNullTipset::TakeNewer,
        )?;
        Ok(tss)
    }
}

pub enum ChainGetGenesis {}
impl RpcMethod<0> for ChainGetGenesis {
    const NAME: &'static str = "Filecoin.ChainGetGenesis";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = ();
    type Ok = Option<Tipset>;

    async fn handle(
        ctx: Ctx,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let genesis = ctx.chain_store().genesis_block_header();
        Ok(Some(Tipset::from(genesis)))
    }
}

pub enum ChainHead {}
impl RpcMethod<0> for ChainHead {
    const NAME: &'static str = "Filecoin.ChainHead";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns the chain head (heaviest tipset).");

    type Params = ();
    type Ok = Tipset;

    async fn handle(
        ctx: Ctx,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let heaviest = ctx.chain_store().heaviest_tipset();
        Ok(heaviest)
    }
}

pub enum ChainGetBlock {}
impl RpcMethod<1> for ChainGetBlock {
    const NAME: &'static str = "Filecoin.ChainGetBlock";
    const PARAM_NAMES: [&'static str; 1] = ["blockCid"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns the block with the specified CID.");

    type Params = (Cid,);
    type Ok = CachingBlockHeader;

    async fn handle(
        ctx: Ctx,
        (block_cid,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let blk: CachingBlockHeader = ctx.db().get_cbor_required(&block_cid)?;
        Ok(blk)
    }
}

pub enum ChainGetTipSet {}

impl RpcMethod<1> for ChainGetTipSet {
    const NAME: &'static str = "Filecoin.ChainGetTipSet";
    const PARAM_NAMES: [&'static str; 1] = ["tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::{ V0 | V1 });
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns the tipset with the specified CID.");

    type Params = (ApiTipsetKey,);
    type Ok = Tipset;

    async fn handle(
        ctx: Ctx,
        (ApiTipsetKey(tsk),): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        if let Some(tsk) = &tsk {
            let ts = ctx.chain_index().load_required_tipset(tsk)?;
            Ok(ts)
        } else {
            // It contains Lotus error message `NewTipSet called with zero length array of blocks` for parity tests
            Err(anyhow::anyhow!(
                "TipsetKey cannot be empty (NewTipSet called with zero length array of blocks)"
            )
            .into())
        }
    }
}

pub enum ChainGetTipSetV2 {}

impl ChainGetTipSetV2 {
    pub async fn get_tipset_by_anchor(
        ctx: &Ctx,
        anchor: Option<&TipsetAnchor>,
    ) -> anyhow::Result<Tipset> {
        if let Some(anchor) = anchor {
            match (&anchor.key.0, &anchor.tag) {
                // Anchor is zero-valued. Fall back to heaviest tipset.
                (None, None) => Ok(ctx.state_manager.heaviest_tipset()),
                // Get tipset at the specified key.
                (Some(tsk), None) => Ok(ctx.chain_index().load_required_tipset(tsk)?),
                (None, Some(tag)) => Self::get_tipset_by_tag(ctx, *tag).await,
                _ => {
                    anyhow::bail!("invalid anchor")
                }
            }
        } else {
            // No anchor specified. Fall back to finalized tipset.
            Self::get_tipset_by_tag(ctx, TipsetTag::Finalized).await
        }
    }

    pub async fn get_tipset_by_tag(ctx: &Ctx, tag: TipsetTag) -> anyhow::Result<Tipset> {
        match tag {
            TipsetTag::Latest => Ok(ctx.state_manager.heaviest_tipset()),
            TipsetTag::Finalized => Self::get_latest_finalized_tipset(ctx).await,
            TipsetTag::Safe => Self::get_latest_safe_tipset(ctx).await,
        }
    }

    pub async fn get_latest_safe_tipset(ctx: &Ctx) -> anyhow::Result<Tipset> {
        let finalized = Self::get_latest_finalized_tipset(ctx).await?;
        let head = ctx.chain_store().heaviest_tipset();
        let safe_height = (head.epoch() - SAFE_HEIGHT_DISTANCE).max(0);
        if finalized.epoch() >= safe_height {
            Ok(finalized)
        } else {
            Ok(ctx.chain_index().load_required_tipset_by_height(
                safe_height,
                head,
                ResolveNullTipset::TakeOlder,
            )?)
        }
    }

    pub async fn get_latest_finalized_tipset(ctx: &Ctx) -> anyhow::Result<Tipset> {
        ChainGetTipSetFinalityStatus::get_finality_status(ctx)?
            .finalized_tip_set
            .context("failed to resolve finalized tipset")
    }

    pub async fn get_tipset(ctx: &Ctx, selector: &TipsetSelector) -> anyhow::Result<Tipset> {
        selector.validate()?;
        // Get tipset by key.
        if let ApiTipsetKey(Some(tsk)) = &selector.key {
            let ts = ctx.chain_index().load_required_tipset(tsk)?;
            return Ok(ts);
        }
        // Get tipset by height.
        if let Some(height) = &selector.height {
            let anchor = Self::get_tipset_by_anchor(ctx, height.anchor.as_ref()).await?;
            let ts = ctx.chain_index().load_required_tipset_by_height(
                height.at,
                anchor,
                height.resolve_null_tipset_policy(),
            )?;
            return Ok(ts);
        }
        // Get tipset by tag, either latest or finalized.
        if let Some(tag) = &selector.tag {
            let ts = Self::get_tipset_by_tag(ctx, *tag).await?;
            return Ok(ts);
        }
        anyhow::bail!("no tipset found for selector")
    }
}

impl RpcMethod<1> for ChainGetTipSetV2 {
    const NAME: &'static str = "Filecoin.ChainGetTipSet";
    const PARAM_NAMES: [&'static str; 1] = ["tipsetSelector"];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::{ V2 });
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns the tipset with the specified CID.");

    type Params = (TipsetSelector,);
    type Ok = Tipset;

    async fn handle(
        ctx: Ctx,
        (selector,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(Self::get_tipset(&ctx, &selector).await?)
    }
}

pub enum ChainGetTipSetFinalityStatus {}

impl ChainGetTipSetFinalityStatus {
    pub fn get_finality_status(ctx: &Ctx) -> anyhow::Result<ChainFinalityStatus> {
        let head = ctx.chain_store().heaviest_tipset();
        let (ec_finality_threshold_depth, ec_finalized_tip_set) =
            Self::get_ec_finality_threshold_depth_and_tipset_with_cache(ctx, head.shallow_clone())?;
        let f3_finalized_tip_set = ctx.chain_store().f3_finalized_tipset();
        let finalized_tip_set = match (&ec_finalized_tip_set, &f3_finalized_tip_set) {
            (Some(ec), Some(f3)) => {
                if ec.epoch() >= f3.epoch() {
                    Some(ec.shallow_clone())
                } else {
                    Some(f3.shallow_clone())
                }
            }
            (Some(ec), None) => Some(ec.shallow_clone()),
            (None, Some(f3)) => Some(f3.shallow_clone()),
            (None, None) => None,
        };
        Ok(ChainFinalityStatus {
            ec_finality_threshold_depth,
            ec_finalized_tip_set,
            f3_finalized_tip_set,
            finalized_tip_set,
            head,
        })
    }

    pub fn get_ec_finality_threshold_depth_and_tipset_with_cache(
        ctx: &Ctx,
        head: Tipset,
    ) -> anyhow::Result<(i64, Option<Tipset>)> {
        static CACHE: parking_lot::Mutex<Option<(Tipset, i64, Option<Tipset>)>> =
            parking_lot::Mutex::new(None);
        let mut cache = CACHE.lock();
        if let Some((cached_head, cached_threshold, cached_tipset)) = &*cache
            && cached_head == &head
        {
            Ok((*cached_threshold, cached_tipset.shallow_clone()))
        } else {
            let (threshold, tipset) =
                Self::get_ec_finality_threshold_depth_and_tipset(ctx, head.shallow_clone())?;
            *cache = Some((head, threshold, tipset.shallow_clone()));
            Ok((threshold, tipset))
        }
    }

    fn get_ec_finality_threshold_depth_and_tipset(
        ctx: &Ctx,
        head: Tipset,
    ) -> anyhow::Result<(i64, Option<Tipset>)> {
        use crate::chain::ec_finality::calculator::{
            DEFAULT_BLOCKS_PER_EPOCH, DEFAULT_BYZANTINE_FRACTION, DEFAULT_GUARANTEE,
            find_threshold_depth,
        };

        /// Number of extra epochs to fetch beyond [`chain_finality`] when
        /// building the chain sample for [`find_threshold_depth`].
        ///
        /// The extra 5 epochs act as a tail buffer to prevent out-of-bounds access,
        /// particularly when null rounds (epochs with zero blocks) are present, since
        /// they consume array slots without advancing the meaningful epoch count.
        const FINALITY_CHAIN_EXTRA_EPOCHS: usize = 5;

        let finality = ctx.chain_config().policy.chain_finality;
        let chain_len = finality as usize + FINALITY_CHAIN_EXTRA_EPOCHS;
        let mut chain = Vec::with_capacity(chain_len);
        let mut ts = head.shallow_clone();
        while chain.len() < chain_len {
            chain.push(ts.len() as i64);
            if let Ok(parent) = ctx.chain_index().load_required_tipset(ts.parents()) {
                // insert 0 for null rounds
                if let Ok(n_null_tipsets_to_pad) = usize::try_from(ts.epoch() - parent.epoch() - 1)
                    && n_null_tipsets_to_pad > 0
                {
                    let target_len =
                        (chain.len().saturating_add(n_null_tipsets_to_pad)).min(chain_len);
                    chain.resize(target_len, 0);
                }
                ts = parent;
            } else {
                break;
            }
        }
        // Reverse to chronological order (oldest first).
        chain.reverse();
        let depth = match find_threshold_depth(
            &chain,
            finality,
            DEFAULT_BLOCKS_PER_EPOCH,
            DEFAULT_BYZANTINE_FRACTION,
            *DEFAULT_GUARANTEE,
        ) {
            Ok(threshold) => threshold,
            Err(e) => {
                tracing::error!(
                    "Failed to calculate EC finality threshold depth: {e:#}, chain: {chain:?}"
                );
                -1
            }
        };
        let finalized = if depth >= 0
            && let Ok(Some(ts)) = ctx.chain_index().tipset_by_height(
                (head.epoch() - depth).max(0),
                head.shallow_clone(),
                ResolveNullTipset::TakeOlder,
            ) {
            Some(ts)
        } else {
            let ec_finality_epoch =
                (head.epoch() - ctx.chain_config().policy.chain_finality).max(0);
            ctx.chain_index().tipset_by_height(
                ec_finality_epoch,
                head,
                ResolveNullTipset::TakeOlder,
            )?
        };
        Ok((depth, finalized))
    }
}

impl RpcMethod<0> for ChainGetTipSetFinalityStatus {
    const NAME: &'static str = "Filecoin.ChainGetTipSetFinalityStatus";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_PATHS: BitFlags<ApiPaths> = make_bitflags!(ApiPaths::{ V2 });
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> =
        Some("Returns a breakdown of how the node is currently determining finality.");

    type Params = ();
    type Ok = ChainFinalityStatus;

    async fn handle(
        ctx: Ctx,
        (): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(Self::get_finality_status(&ctx)?)
    }
}

pub enum ChainSetHead {}
impl RpcMethod<1> for ChainSetHead {
    const NAME: &'static str = "Filecoin.ChainSetHead";
    const PARAM_NAMES: [&'static str; 1] = ["tsk"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Admin;

    type Params = (TipsetKey,);
    type Ok = ();

    async fn handle(
        ctx: Ctx,
        (tsk,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        // This is basically a port of the reference implementation at
        // https://github.com/filecoin-project/lotus/blob/v1.23.0/node/impl/full/chain.go#L321

        let new_head = ctx.chain_index().load_required_tipset(&tsk)?;
        let mut current = ctx.chain_store().heaviest_tipset();
        while current.epoch() >= new_head.epoch() {
            for cid in current.key().to_cids() {
                ctx.chain_store().unmark_block_as_validated(&cid);
            }
            let parents = &current.block_headers().first().parents;
            current = ctx.chain_index().load_required_tipset(parents)?;
        }
        ctx.chain_store()
            .set_heaviest_tipset(new_head)
            .map_err(Into::into)
    }
}

pub enum ChainGetMinBaseFee {}
impl RpcMethod<1> for ChainGetMinBaseFee {
    const NAME: &'static str = "Forest.ChainGetMinBaseFee";
    const PARAM_NAMES: [&'static str; 1] = ["lookback"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (u32,);
    type Ok = String;

    async fn handle(
        ctx: Ctx,
        (lookback,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let mut current = ctx.chain_store().heaviest_tipset();
        let mut min_base_fee = current.block_headers().first().parent_base_fee.clone();

        for _ in 0..lookback {
            let parents = &current.block_headers().first().parents;
            current = ctx.chain_index().load_required_tipset(parents)?;

            min_base_fee =
                min_base_fee.min(current.block_headers().first().parent_base_fee.to_owned());
        }

        Ok(min_base_fee.atto().to_string())
    }
}

pub enum ChainTipSetWeight {}
impl RpcMethod<1> for ChainTipSetWeight {
    const NAME: &'static str = "Filecoin.ChainTipSetWeight";
    const PARAM_NAMES: [&'static str; 1] = ["tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns the weight of the specified tipset.");

    type Params = (ApiTipsetKey,);
    type Ok = BigInt;

    async fn handle(
        ctx: Ctx,
        (ApiTipsetKey(tipset_key),): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx
            .chain_store()
            .load_required_tipset_or_heaviest(&tipset_key)?;
        let weight = crate::fil_cns::weight(ctx.db(), &ts)?;
        Ok(weight)
    }
}

pub enum ChainGetTipsetByParentState {}
impl RpcMethod<1> for ChainGetTipsetByParentState {
    const NAME: &'static str = "Forest.ChainGetTipsetByParentState";
    const PARAM_NAMES: [&'static str; 1] = ["parentState"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (Cid,);
    type Ok = Option<Tipset>;

    async fn handle(
        ctx: Ctx,
        (parent_state,): Self::Params,
        _: &http::Extensions,
    ) -> Result<Self::Ok, ServerError> {
        Ok(ctx
            .chain_store()
            .heaviest_tipset()
            .chain(ctx.db())
            .find(|ts| ts.parent_state() == &parent_state)
            .shallow_clone())
    }
}

pub const CHAIN_NOTIFY: &str = "Filecoin.ChainNotify";
pub(crate) fn chain_notify(
    _params: Params<'_>,
    data: &crate::rpc::RPCState,
) -> Subscriber<Vec<ApiHeadChange>> {
    let (sender, receiver) = broadcast::channel(HEAD_CHANNEL_CAPACITY);

    // As soon as the channel is created, send the current tipset
    let current = data.chain_store().heaviest_tipset();
    let (change, tipset) = ("current".into(), current);
    sender
        .send(vec![ApiHeadChange { change, tipset }])
        .expect("receiver is not dropped");

    let mut head_changes_rx = data.chain_store().subscribe_head_changes();

    tokio::spawn(async move {
        // Skip first message
        let _ = head_changes_rx.recv().await;
        while let Ok(changes) = head_changes_rx.recv().await {
            let api_changes = changes
                .into_change_vec()
                .into_iter()
                .map(From::from)
                .collect();
            if sender.send(api_changes).is_err() {
                break;
            }
        }
    });
    receiver
}

async fn load_api_messages_from_tipset(
    ctx: &crate::rpc::RPCState,
    tipset_keys: &TipsetKey,
) -> Result<Vec<ApiMessage>, ServerError> {
    static SHOULD_BACKFILL: LazyLock<bool> = LazyLock::new(|| {
        let enabled = is_env_truthy("FOREST_RPC_BACKFILL_FULL_TIPSET_FROM_NETWORK");
        if enabled {
            tracing::warn!(
                "Full tipset backfilling from network is enabled via FOREST_RPC_BACKFILL_FULL_TIPSET_FROM_NETWORK, excessive disk and bandwidth usage is expected."
            );
        }
        enabled
    });
    let full_tipset = if *SHOULD_BACKFILL {
        get_full_tipset(
            &ctx.sync_network_context,
            ctx.chain_store(),
            None,
            tipset_keys,
        )
        .await?
    } else {
        load_full_tipset(ctx.chain_store(), tipset_keys)?
    };
    let blocks = full_tipset.into_blocks();
    let mut messages = vec![];
    let mut seen = CidHashSet::default();
    for Block {
        bls_messages,
        secp_messages,
        ..
    } in blocks
    {
        for message in bls_messages {
            let cid = message.cid();
            if seen.insert(cid) {
                messages.push(ApiMessage { cid, message });
            }
        }

        for msg in secp_messages {
            let cid = msg.cid();
            if seen.insert(cid) {
                messages.push(ApiMessage {
                    cid,
                    message: msg.message,
                });
            }
        }
    }

    Ok(messages)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BlockMessages {
    #[serde(rename = "BlsMessages", with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Vec<Message>>")]
    pub bls_msg: Vec<Message>,
    #[serde(rename = "SecpkMessages", with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Vec<SignedMessage>>")]
    pub secp_msg: Vec<SignedMessage>,
    #[serde(rename = "Cids", with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Vec<Cid>>")]
    pub cids: Vec<Cid>,
}
lotus_json_with_self!(BlockMessages);

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct ApiReceipt {
    // Exit status of message execution
    pub exit_code: ExitCode,
    // `Return` value if the exit code is zero
    #[serde(rename = "Return", with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<RawBytes>")]
    pub return_data: RawBytes,
    // Non-negative value of GasUsed
    pub gas_used: u64,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Option<Cid>>")]
    pub events_root: Option<Cid>,
}

lotus_json_with_self!(ApiReceipt);

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct ApiMessage {
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Cid>")]
    pub cid: Cid,
    #[serde(with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Message>")]
    pub message: Message,
}

lotus_json_with_self!(ApiMessage);

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ForestChainExportParams {
    pub version: FilecoinSnapshotVersion,
    pub epoch: ChainEpoch,
    pub recent_roots: i64,
    pub output_path: PathBuf,
    #[schemars(with = "LotusJson<ApiTipsetKey>")]
    #[serde(with = "crate::lotus_json")]
    pub tipset_keys: ApiTipsetKey,
    #[serde(default)]
    pub include_receipts: bool,
    #[serde(default)]
    pub include_events: bool,
    #[serde(default)]
    pub include_tipset_keys: bool,
    pub skip_checksum: bool,
    pub dry_run: bool,
}
lotus_json_with_self!(ForestChainExportParams);

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ForestChainExportDiffParams {
    pub from: ChainEpoch,
    pub to: ChainEpoch,
    pub depth: i64,
    pub output_path: PathBuf,
}
lotus_json_with_self!(ForestChainExportDiffParams);

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChainExportParams {
    pub epoch: ChainEpoch,
    pub recent_roots: i64,
    pub output_path: PathBuf,
    #[schemars(with = "LotusJson<ApiTipsetKey>")]
    #[serde(with = "crate::lotus_json")]
    pub tipset_keys: ApiTipsetKey,
    pub skip_checksum: bool,
    pub dry_run: bool,
}
lotus_json_with_self!(ChainExportParams);

#[derive(PartialEq, Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct ApiHeadChange {
    #[serde(rename = "Type")]
    pub change: String,
    #[serde(rename = "Val", with = "crate::lotus_json")]
    #[schemars(with = "LotusJson<Tipset>")]
    pub tipset: Tipset,
}
lotus_json_with_self!(ApiHeadChange);

impl From<HeadChange> for ApiHeadChange {
    fn from(change: HeadChange) -> Self {
        match change {
            HeadChange::Apply(tipset) => Self {
                change: "apply".into(),
                tipset,
            },
            HeadChange::Revert(tipset) => Self {
                change: "revert".into(),
                tipset,
            },
        }
    }
}

#[derive(PartialEq, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "Type", content = "Val", rename_all = "snake_case")]
pub enum PathChange<T = Tipset> {
    Revert(T),
    Apply(T),
}

impl<T: Clone> Clone for PathChange<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Revert(i) => Self::Revert(i.clone()),
            Self::Apply(i) => Self::Apply(i.clone()),
        }
    }
}

impl HasLotusJson for PathChange {
    type LotusJson = PathChange<<Tipset as HasLotusJson>::LotusJson>;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        use crate::test_utils::dummy_ticket;
        use serde_json::json;
        let header = CachingBlockHeader::new(RawBlockHeader {
            ticket: dummy_ticket(0),
            ..Default::default()
        });
        let header_cid = *header.cid();
        vec![(
            json!({
                "Type": "revert",
                "Val": {
                    "Blocks": [
                        {
                            "BeaconEntries": null,
                            "ForkSignaling": 0,
                            "Height": 0,
                            "Messages": { "/": "baeaaaaa" },
                            "Miner": "f00",
                            "ParentBaseFee": "0",
                            "ParentMessageReceipts": { "/": "baeaaaaa" },
                            "ParentStateRoot": { "/":"baeaaaaa" },
                            "ParentWeight": "0",
                            "Parents": [{"/":"bafyreiaqpwbbyjo4a42saasj36kkrpv4tsherf2e7bvezkert2a7dhonoi"}],
                            "Ticket": { "VRFProof": "AA==" },
                            "Timestamp": 0,
                            "WinPoStProof": null
                        }
                    ],
                    "Cids": [
                        { "/": header_cid.to_string() }
                    ],
                    "Height": 0
                }
            }),
            Self::Revert(Tipset::from(header)),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        match self {
            PathChange::Revert(it) => PathChange::Revert(it.into_lotus_json()),
            PathChange::Apply(it) => PathChange::Apply(it.into_lotus_json()),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        match lotus_json {
            PathChange::Revert(it) => PathChange::Revert(Tipset::from_lotus_json(it)),
            PathChange::Apply(it) => PathChange::Apply(Tipset::from_lotus_json(it)),
        }
    }
}

#[derive(Debug)]
pub struct PathChanges<T = Tipset> {
    pub reverts: Vec<T>,
    pub applies: Vec<T>,
}

impl<T: Clone> Clone for PathChanges<T> {
    fn clone(&self) -> Self {
        let Self { reverts, applies } = self;
        Self {
            reverts: reverts.clone(),
            applies: applies.clone(),
        }
    }
}

impl<T> PathChanges<T> {
    pub fn into_change_vec(self) -> Vec<PathChange<T>> {
        let Self { reverts, applies } = self;
        reverts
            .into_iter()
            .map(PathChange::Revert)
            .chain(applies.into_iter().map(PathChange::Apply))
            .collect_vec()
    }
}

#[cfg(test)]
impl<T> quickcheck::Arbitrary for PathChange<T>
where
    T: quickcheck::Arbitrary + ShallowClone,
{
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        let inner = T::arbitrary(g);
        g.choose(&[PathChange::Apply(inner.clone()), PathChange::Revert(inner)])
            .unwrap()
            .clone()
    }
}

#[test]
fn snapshots() {
    assert_all_snapshots::<PathChange>()
}

#[cfg(test)]
#[quickcheck_macros::quickcheck]
fn quickcheck(val: PathChange) {
    assert_unchanged_via_json(val)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        blocks::{Chain4U, RawBlockHeader, chain4u},
        db::{
            MemoryDB,
            car::{AnyCar, ManyCar},
        },
        networks::{self, ChainConfig},
    };
    use PathChange::{Apply, Revert};
    use std::sync::Arc;

    #[test]
    fn revert_to_ancestor_linear() {
        let cs = ChainStore::calibnet();
        let db = Chain4U::with_blockstore(cs.db_owned());
        chain4u! {
            in db;
            [_genesis = cs.genesis_block_header()]
            -> [a] -> [b] -> [c, d] -> [e]
        };

        // simple
        assert_path_change(&cs, b, a, [Revert(&[b])]);

        // from multi-member tipset
        assert_path_change(&cs, [c, d], a, [Revert(&[c, d][..]), Revert(&[b])]);

        // to multi-member tipset
        assert_path_change(&cs, e, [c, d], [Revert(e)]);

        // over multi-member tipset
        assert_path_change(&cs, e, b, [Revert(&[e][..]), Revert(&[c, d])]);
    }

    /// Mirror how lotus handles passing an incomplete `TipsetKey`s.
    /// Tested on lotus `1.23.2`
    #[test]
    fn incomplete_tipsets() {
        let cs = ChainStore::calibnet();
        let db = Chain4U::with_blockstore(cs.db_owned());
        chain4u! {
            in db;
            [_genesis = cs.genesis_block_header()]
            -> [a, b] -> [c] -> [d, _e] // this pattern 2 -> 1 -> 2 can be found at calibnet epoch 1369126
        };

        // apply to descendant with incomplete `from`
        assert_path_change(
            &cs,
            a,
            c,
            [
                Revert(&[a][..]), // revert the incomplete tipset
                Apply(&[a, b]),   // apply the complete one
                Apply(&[c]),      // apply the destination
            ],
        );

        // apply to descendant with incomplete `to`
        assert_path_change(&cs, c, d, [Apply(d)]);

        // revert to ancestor with incomplete `from`
        assert_path_change(&cs, d, c, [Revert(d)]);

        // revert to ancestor with incomplete `to`
        assert_path_change(
            &cs,
            c,
            a,
            [
                Revert(&[c][..]),
                Revert(&[a, b]), // revert the complete tipset
                Apply(&[a]),     // apply the incomplete one
            ],
        );
    }

    #[test]
    fn apply_to_descendant_linear() {
        let cs = ChainStore::calibnet();
        let db = Chain4U::with_blockstore(cs.db_owned());
        chain4u! {
            in db;
            [_genesis = cs.genesis_block_header()]
            -> [a] -> [b] -> [c, d] -> [e]
        };

        // simple
        assert_path_change(&cs, a, b, [Apply(&[b])]);

        // from multi-member tipset
        assert_path_change(&cs, [c, d], e, [Apply(e)]);

        // to multi-member tipset
        assert_path_change(&cs, b, [c, d], [Apply([c, d])]);

        // over multi-member tipset
        assert_path_change(&cs, b, e, [Apply(&[c, d][..]), Apply(&[e])]);
    }

    #[test]
    fn cross_fork_simple() {
        let cs = ChainStore::calibnet();
        let db = Chain4U::with_blockstore(cs.db_owned());
        chain4u! {
            in db;
            [_genesis = cs.genesis_block_header()]
            -> [a] -> [b1] -> [c1]
        };
        chain4u! {
            from [a] in db;
            [b2] -> [c2]
        };

        // same height
        assert_path_change(&cs, b1, b2, [Revert(b1), Apply(b2)]);

        // different height
        assert_path_change(&cs, b1, c2, [Revert(b1), Apply(b2), Apply(c2)]);

        let _ = (a, c1);
    }

    impl ChainStore {
        fn _load(genesis_car: &'static [u8], genesis_cid: Cid) -> Self {
            let db = Arc::new(
                ManyCar::new(MemoryDB::default())
                    .with_read_only(AnyCar::new(genesis_car).unwrap())
                    .unwrap(),
            );
            let genesis_block_header = db.get_cbor(&genesis_cid).unwrap().unwrap();
            ChainStore::new(db, Arc::new(ChainConfig::calibnet()), genesis_block_header).unwrap()
        }
        pub fn calibnet() -> Self {
            Self::_load(
                networks::calibnet::DEFAULT_GENESIS,
                *networks::calibnet::GENESIS_CID,
            )
        }
    }

    /// Utility for writing ergonomic tests
    trait MakeTipset {
        fn make_tipset(self) -> Tipset;
    }

    impl MakeTipset for &RawBlockHeader {
        fn make_tipset(self) -> Tipset {
            Tipset::from(CachingBlockHeader::new(self.clone()))
        }
    }

    impl<const N: usize> MakeTipset for [&RawBlockHeader; N] {
        fn make_tipset(self) -> Tipset {
            self.as_slice().make_tipset()
        }
    }

    impl<const N: usize> MakeTipset for &[&RawBlockHeader; N] {
        fn make_tipset(self) -> Tipset {
            self.as_slice().make_tipset()
        }
    }

    impl MakeTipset for &[&RawBlockHeader] {
        fn make_tipset(self) -> Tipset {
            Tipset::new(self.iter().cloned().cloned()).unwrap()
        }
    }

    #[track_caller]
    fn assert_path_change<T: MakeTipset>(
        store: &ChainStore,
        from: impl MakeTipset,
        to: impl MakeTipset,
        expected: impl IntoIterator<Item = PathChange<T>>,
    ) {
        fn print(path_change: &PathChange) {
            let it = match path_change {
                Revert(it) => {
                    print!("Revert(");
                    it
                }
                Apply(it) => {
                    print!(" Apply(");
                    it
                }
            };
            println!(
                "epoch = {}, key.cid = {})",
                it.epoch(),
                it.key().cid().unwrap()
            )
        }

        let actual = chain_get_path(store, from.make_tipset().key(), to.make_tipset().key())
            .unwrap()
            .into_change_vec();
        let expected = expected
            .into_iter()
            .map(|change| match change {
                PathChange::Revert(it) => PathChange::Revert(it.make_tipset()),
                PathChange::Apply(it) => PathChange::Apply(it.make_tipset()),
            })
            .collect_vec();
        if expected != actual {
            println!("SUMMARY");
            println!("=======");
            println!("expected:");
            for it in &expected {
                print(it)
            }
            println!();
            println!("actual:");
            for it in &actual {
                print(it)
            }
            println!("=======\n")
        }
        assert_eq!(
            expected, actual,
            "expected change (left) does not match actual change (right)"
        )
    }
}
