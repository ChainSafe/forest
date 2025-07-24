// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod types;
use enumflags2::BitFlags;
use types::*;

#[cfg(test)]
use crate::blocks::RawBlockHeader;
use crate::blocks::{Block, CachingBlockHeader, Tipset, TipsetKey};
use crate::chain::index::ResolveNullTipset;
use crate::chain::{ChainStore, ExportOptions, HeadChange};
use crate::cid_collections::CidHashSet;
use crate::ipld::DfsIter;
use crate::lotus_json::{HasLotusJson, LotusJson, lotus_json_with_self};
#[cfg(test)]
use crate::lotus_json::{assert_all_snapshots, assert_unchanged_via_json};
use crate::message::{ChainMessage, SignedMessage};
use crate::rpc::types::{ApiTipsetKey, Event};
use crate::rpc::{ApiPaths, Ctx, EthEventHandler, Permission, RpcMethod, ServerError};
use crate::shim::clock::ChainEpoch;
use crate::shim::error::ExitCode;
use crate::shim::executor::Receipt;
use crate::shim::message::Message;
use crate::utils::db::CborStoreExt as _;
use crate::utils::io::VoidAsyncWriter;
use anyhow::{Context as _, Result};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{CborStore, RawBytes};
use hex::ToHex;
use ipld_core::ipld::Ipld;
use jsonrpsee::types::Params;
use jsonrpsee::types::error::ErrorObjectOwned;
use num::BigInt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, LazyLock},
};
use tokio::sync::{
    Mutex,
    broadcast::{self, Receiver as Subscriber},
};

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
        ctx: Ctx<impl Blockstore>,
        (message_cid,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let chain_message: ChainMessage = ctx
            .store()
            .get_cbor(&message_cid)?
            .with_context(|| format!("can't find message with cid {message_cid}"))?;
        Ok(match chain_message {
            ChainMessage::Signed(m) => m.into_message(),
            ChainMessage::Unsigned(m) => m,
        })
    }
}

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
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (root_cid,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let tsk = ctx
            .state_manager
            .chain_store()
            .get_tipset_key(&root_cid)?
            .with_context(|| format!("can't find events with cid {root_cid}"))?;

        let ts = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;

        let events = EthEventHandler::collect_chain_events(&ctx, &ts, &root_cid).await?;

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
        ctx: Ctx<impl Blockstore>,
        (block_cid,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let store = ctx.store();
        let block_header: CachingBlockHeader = store
            .get_cbor(&block_cid)?
            .with_context(|| format!("can't find block header with cid {block_cid}"))?;
        if block_header.epoch == 0 {
            Ok(vec![])
        } else {
            let parent_tipset = Tipset::load_required(store, &block_header.parents)?;
            load_api_messages_from_tipset(store, &parent_tipset)
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
        ctx: Ctx<impl Blockstore>,
        (block_cid,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let store = ctx.store();
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
            .collect();

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
        ctx: Ctx<impl Blockstore>,
        (ApiTipsetKey(tipset_key),): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let tipset = ctx
            .chain_store()
            .load_required_tipset_or_heaviest(&tipset_key)?;
        load_api_messages_from_tipset(ctx.store(), &tipset)
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
        _ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (blocking,): Self::Params,
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
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (ForestChainExportParams,);
    type Ok = Option<String>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (params,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ForestChainExportParams {
            epoch,
            recent_roots,
            output_path,
            tipset_keys: ApiTipsetKey(tsk),
            unordered,
            skip_checksum,
            dry_run,
        } = params;

        static LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

        let _locked = LOCK.try_lock();
        if _locked.is_err() {
            return Err(anyhow::anyhow!("Another chain export job is still in progress").into());
        }

        let chain_finality = ctx.chain_config().policy.chain_finality;
        if recent_roots < chain_finality {
            return Err(anyhow::anyhow!(format!(
                "recent-stateroots must be greater than {chain_finality}"
            ))
            .into());
        }

        let head = ctx.chain_store().load_required_tipset_or_heaviest(&tsk)?;
        let start_ts =
            ctx.chain_index()
                .tipset_by_height(epoch, head, ResolveNullTipset::TakeOlder)?;

        let option = Some(ExportOptions {
            skip_checksum,
            unordered,
            ..Default::default()
        });
        match if dry_run {
            crate::chain::export::<Sha256>(
                &ctx.store_owned(),
                &start_ts,
                recent_roots,
                VoidAsyncWriter,
                option,
            )
            .await
        } else {
            let file = tokio::fs::File::create(&output_path).await?;
            crate::chain::export::<Sha256>(
                &ctx.store_owned(),
                &start_ts,
                recent_roots,
                file,
                option,
            )
            .await
        } {
            Ok(checksum_opt) => Ok(checksum_opt.map(|hash| hash.encode_hex())),
            Err(e) => Err(anyhow::anyhow!(e).into()),
        }
    }
}

pub enum ChainExport {}
impl RpcMethod<1> for ChainExport {
    const NAME: &'static str = "Filecoin.ChainExport";
    const PARAM_NAMES: [&'static str; 1] = ["params"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;

    type Params = (ChainExportParams,);
    type Ok = Option<String>;

    async fn handle(
        ctx: Ctx<impl Blockstore + Send + Sync + 'static>,
        (params,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ChainExportParams {
            epoch,
            recent_roots,
            output_path,
            tipset_keys,
            skip_checksum,
            dry_run,
        } = params;

        ForestChainExport::handle(
            ctx,
            (ForestChainExportParams {
                unordered: false,
                epoch,
                recent_roots,
                output_path,
                tipset_keys,
                skip_checksum,
                dry_run,
            },),
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
        ctx: Ctx<impl Blockstore>,
        (cid,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let bytes = ctx
            .store()
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
        ctx: Ctx<impl Blockstore>,
        (cid,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        Ok(ctx.store().get(&cid)?.is_some())
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
        ctx: Ctx<impl Blockstore>,
        (obj_cid, base_cid): Self::Params,
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
                let data = ctx.store().get(&link_cid)?;
                if let Some(data) = data {
                    if collect {
                        stats.links += 1;
                        stats.size += data.len();
                    }
                    if matches!(link_cid.codec(), fvm_ipld_encoding::DAG_CBOR) {
                        if let Ok(ipld) =
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
        ctx: Ctx<impl Blockstore>,
        (block_cid,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let blk: CachingBlockHeader = ctx.store().get_cbor_required(&block_cid)?;
        let blk_msgs = &blk.messages;
        let (unsigned_cids, signed_cids) = crate::chain::read_msg_cids(ctx.store(), blk_msgs)?;
        let (bls_msg, secp_msg) =
            crate::chain::block_messages_from_cids(ctx.store(), &unsigned_cids, &signed_cids)?;
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
        ctx: Ctx<impl Blockstore>,
        (from, to): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        impl_chain_get_path(ctx.chain_store(), &from, &to).map_err(Into::into)
    }
}

/// Find the path between two tipsets, as a series of [`PathChange`]s.
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
fn impl_chain_get_path(
    chain_store: &ChainStore<impl Blockstore>,
    from: &TipsetKey,
    to: &TipsetKey,
) -> anyhow::Result<Vec<PathChange>> {
    let mut to_revert = chain_store
        .load_required_tipset_or_heaviest(from)
        .context("couldn't load `from`")?;
    let mut to_apply = chain_store
        .load_required_tipset_or_heaviest(to)
        .context("couldn't load `to`")?;

    let mut all_reverts = vec![];
    let mut all_applies = vec![];

    // This loop is guaranteed to terminate if the blockstore contain no cycles.
    // This is currently computationally infeasible.
    while to_revert != to_apply {
        if to_revert.epoch() > to_apply.epoch() {
            let next = chain_store
                .load_required_tipset_or_heaviest(to_revert.parents())
                .context("couldn't load ancestor of `from`")?;
            all_reverts.push(to_revert);
            to_revert = next;
        } else {
            let next = chain_store
                .load_required_tipset_or_heaviest(to_apply.parents())
                .context("couldn't load ancestor of `to`")?;
            all_applies.push(to_apply);
            to_apply = next;
        }
    }
    Ok(all_reverts
        .into_iter()
        .map(PathChange::Revert)
        .chain(all_applies.into_iter().rev().map(PathChange::Apply))
        .collect())
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
        ctx: Ctx<impl Blockstore>,
        (height, ApiTipsetKey(tipset_key)): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx
            .chain_store()
            .load_required_tipset_or_heaviest(&tipset_key)?;
        let tss = ctx
            .chain_index()
            .tipset_by_height(height, ts, ResolveNullTipset::TakeOlder)?;
        Ok((*tss).clone())
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
        ctx: Ctx<impl Blockstore>,
        (height, ApiTipsetKey(tipset_key)): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx
            .chain_store()
            .load_required_tipset_or_heaviest(&tipset_key)?;
        let tss = ctx
            .chain_index()
            .tipset_by_height(height, ts, ResolveNullTipset::TakeNewer)?;
        Ok((*tss).clone())
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

    async fn handle(ctx: Ctx<impl Blockstore>, (): Self::Params) -> Result<Self::Ok, ServerError> {
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

    async fn handle(ctx: Ctx<impl Blockstore>, (): Self::Params) -> Result<Self::Ok, ServerError> {
        let heaviest = ctx.chain_store().heaviest_tipset();
        Ok((*heaviest).clone())
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
        ctx: Ctx<impl Blockstore>,
        (block_cid,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let blk: CachingBlockHeader = ctx.store().get_cbor_required(&block_cid)?;
        Ok(blk)
    }
}

pub enum ChainGetTipSet {}
impl RpcMethod<1> for ChainGetTipSet {
    const NAME: &'static str = "Filecoin.ChainGetTipSet";
    const PARAM_NAMES: [&'static str; 1] = ["tipsetKey"];
    const API_PATHS: BitFlags<ApiPaths> = ApiPaths::all();
    const PERMISSION: Permission = Permission::Read;
    const DESCRIPTION: Option<&'static str> = Some("Returns the tipset with the specified CID.");

    type Params = (ApiTipsetKey,);
    type Ok = Tipset;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (ApiTipsetKey(tipset_key),): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx
            .chain_store()
            .load_required_tipset_or_heaviest(&tipset_key)?;
        Ok((*ts).clone())
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
        ctx: Ctx<impl Blockstore>,
        (tsk,): Self::Params,
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
        ctx: Ctx<impl Blockstore>,
        (lookback,): Self::Params,
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
        ctx: Ctx<impl Blockstore>,
        (ApiTipsetKey(tipset_key),): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx
            .chain_store()
            .load_required_tipset_or_heaviest(&tipset_key)?;
        let weight = crate::fil_cns::weight(ctx.store(), &ts)?;
        Ok(weight)
    }
}

pub const CHAIN_NOTIFY: &str = "Filecoin.ChainNotify";
pub(crate) fn chain_notify<DB: Blockstore>(
    _params: Params<'_>,
    data: &crate::rpc::RPCState<DB>,
) -> Subscriber<Vec<ApiHeadChange>> {
    let (sender, receiver) = broadcast::channel(100);

    // As soon as the channel is created, send the current tipset
    let current = data.chain_store().heaviest_tipset();
    let (change, tipset) = ("current".into(), current);
    sender
        .send(vec![ApiHeadChange {
            change,
            tipset: tipset.as_ref().clone(),
        }])
        .expect("receiver is not dropped");

    let mut subscriber = data.chain_store().publisher().subscribe();

    tokio::spawn(async move {
        // Skip first message
        let _ = subscriber.recv().await;

        while let Ok(v) = subscriber.recv().await {
            let (change, tipset) = match v {
                HeadChange::Apply(ts) => ("apply".into(), ts),
            };

            if sender
                .send(vec![ApiHeadChange {
                    change,
                    tipset: tipset.as_ref().clone(),
                }])
                .is_err()
            {
                break;
            }
        }
    });
    receiver
}

fn load_api_messages_from_tipset(
    store: &impl Blockstore,
    tipset: &Tipset,
) -> Result<Vec<ApiMessage>, ServerError> {
    let full_tipset = tipset
        .fill_from_blockstore(store)
        .context("Failed to load full tipset")?;
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
    pub epoch: ChainEpoch,
    pub recent_roots: i64,
    pub output_path: PathBuf,
    #[schemars(with = "LotusJson<ApiTipsetKey>")]
    #[serde(with = "crate::lotus_json")]
    pub tipset_keys: ApiTipsetKey,
    pub unordered: bool,
    pub skip_checksum: bool,
    pub dry_run: bool,
}
lotus_json_with_self!(ForestChainExportParams);

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

#[derive(PartialEq, Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(tag = "Type", content = "Val", rename_all = "snake_case")]
pub enum PathChange<T = Arc<Tipset>> {
    Revert(T),
    Apply(T),
}
impl HasLotusJson for PathChange {
    type LotusJson = PathChange<<Tipset as HasLotusJson>::LotusJson>;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        use serde_json::json;
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
                            "Timestamp": 0,
                            "WinPoStProof": null
                        }
                    ],
                    "Cids": [
                        { "/": "bafy2bzaceag62hjj3o43lf6oyeox3fvg5aqkgl5zagbwpjje3ajwg6yw4iixk" }
                    ],
                    "Height": 0
                }
            }),
            Self::Revert(Arc::new(Tipset::from(RawBlockHeader::default()))),
        )]
    }

    fn into_lotus_json(self) -> Self::LotusJson {
        match self {
            PathChange::Revert(it) => {
                PathChange::Revert(Arc::unwrap_or_clone(it).into_lotus_json())
            }
            PathChange::Apply(it) => PathChange::Apply(Arc::unwrap_or_clone(it).into_lotus_json()),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        match lotus_json {
            PathChange::Revert(it) => PathChange::Revert(Tipset::from_lotus_json(it).into()),
            PathChange::Apply(it) => PathChange::Apply(Tipset::from_lotus_json(it).into()),
        }
    }
}

#[cfg(test)]
impl<T> quickcheck::Arbitrary for PathChange<T>
where
    T: quickcheck::Arbitrary,
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
quickcheck::quickcheck! {
    fn quickcheck(val: PathChange) -> () {
        assert_unchanged_via_json(val)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use PathChange::{Apply, Revert};

    use crate::{
        blocks::{Chain4U, RawBlockHeader, chain4u},
        db::{MemoryDB, car::PlainCar},
        networks::{self, ChainConfig},
    };

    #[test]
    fn revert_to_ancestor_linear() {
        let store = ChainStore::calibnet();
        chain4u! {
            in store.blockstore();
            [_genesis = store.genesis_block_header()]
            -> [a] -> [b] -> [c, d] -> [e]
        };

        // simple
        assert_path_change(&store, b, a, [Revert(&[b])]);

        // from multi-member tipset
        assert_path_change(&store, [c, d], a, [Revert(&[c, d][..]), Revert(&[b])]);

        // to multi-member tipset
        assert_path_change(&store, e, [c, d], [Revert(e)]);

        // over multi-member tipset
        assert_path_change(&store, e, b, [Revert(&[e][..]), Revert(&[c, d])]);
    }

    /// Mirror how lotus handles passing an incomplete `TipsetKey`s.
    /// Tested on lotus `1.23.2`
    #[test]
    fn incomplete_tipsets() {
        let store = ChainStore::calibnet();
        chain4u! {
            in store.blockstore();
            [_genesis = store.genesis_block_header()]
            -> [a, b] -> [c] -> [d, _e] // this pattern 2 -> 1 -> 2 can be found at calibnet epoch 1369126
        };

        // apply to descendant with incomplete `from`
        assert_path_change(
            &store,
            a,
            c,
            [
                Revert(&[a][..]), // revert the incomplete tipset
                Apply(&[a, b]),   // apply the complete one
                Apply(&[c]),      // apply the destination
            ],
        );

        // apply to descendant with incomplete `to`
        assert_path_change(&store, c, d, [Apply(d)]);

        // revert to ancestor with incomplete `from`
        assert_path_change(&store, d, c, [Revert(d)]);

        // revert to ancestor with incomplete `to`
        assert_path_change(
            &store,
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
        let store = ChainStore::calibnet();
        chain4u! {
            in store.blockstore();
            [_genesis = store.genesis_block_header()]
            -> [a] -> [b] -> [c, d] -> [e]
        };

        // simple
        assert_path_change(&store, a, b, [Apply(&[b])]);

        // from multi-member tipset
        assert_path_change(&store, [c, d], e, [Apply(e)]);

        // to multi-member tipset
        assert_path_change(&store, b, [c, d], [Apply([c, d])]);

        // over multi-member tipset
        assert_path_change(&store, b, e, [Apply(&[c, d][..]), Apply(&[e])]);
    }

    #[test]
    fn cross_fork_simple() {
        let store = ChainStore::calibnet();
        chain4u! {
            in store.blockstore();
            [_genesis = store.genesis_block_header()]
            -> [a] -> [b1] -> [c1]
        };
        chain4u! {
            from [a] in store.blockstore();
            [b2] -> [c2]
        };

        // same height
        assert_path_change(&store, b1, b2, [Revert(b1), Apply(b2)]);

        // different height
        assert_path_change(&store, b1, c2, [Revert(b1), Apply(b2), Apply(c2)]);

        let _ = (a, c1);
    }

    impl ChainStore<Chain4U<PlainCar<&'static [u8]>>> {
        fn _load(genesis_car: &'static [u8], genesis_cid: Cid) -> Self {
            let db = Arc::new(Chain4U::with_blockstore(
                PlainCar::new(genesis_car).unwrap(),
            ));
            let genesis_block_header = db.get_cbor(&genesis_cid).unwrap().unwrap();
            ChainStore::new(
                db,
                Arc::new(MemoryDB::default()),
                Arc::new(MemoryDB::default()),
                Arc::new(MemoryDB::default()),
                Arc::new(ChainConfig::calibnet()),
                genesis_block_header,
            )
            .unwrap()
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
        store: &ChainStore<impl Blockstore>,
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

        let actual =
            impl_chain_get_path(store, from.make_tipset().key(), to.make_tipset().key()).unwrap();
        let expected = expected
            .into_iter()
            .map(|change| match change {
                PathChange::Revert(it) => PathChange::Revert(Arc::new(it.make_tipset())),
                PathChange::Apply(it) => PathChange::Apply(Arc::new(it.make_tipset())),
            })
            .collect::<Vec<_>>();
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
