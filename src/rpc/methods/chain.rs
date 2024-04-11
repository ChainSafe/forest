// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

#[cfg(test)]
use crate::blocks::RawBlockHeader;
use crate::blocks::{CachingBlockHeader, Tipset, TipsetKey};
use crate::chain::index::ResolveNullTipset;
use crate::chain::{ChainStore, HeadChange};
use crate::cid_collections::CidHashSet;
use crate::lotus_json::lotus_json_with_self;
use crate::lotus_json::HasLotusJson;
use crate::lotus_json::LotusJson;
#[cfg(test)]
use crate::lotus_json::{assert_all_snapshots, assert_unchanged_via_json};
use crate::message::{ChainMessage, SignedMessage};
use crate::rpc::types::ApiTipsetKey;
use crate::rpc::{ApiVersion, Ctx, RpcMethod, ServerError};
use crate::shim::clock::ChainEpoch;
use crate::shim::error::ExitCode;
use crate::shim::message::Message;
use crate::utils::io::VoidAsyncWriter;
use anyhow::{Context as _, Result};
use cid::Cid;
use fil_actors_shared::fvm_ipld_amt::Amtv0 as Amt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{CborStore, RawBytes};
use hex::ToHex;
use jsonrpsee::types::error::ErrorObjectOwned;
use jsonrpsee::types::Params;
use once_cell::sync::Lazy;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{
    broadcast::{self, Receiver as Subscriber},
    Mutex,
};

macro_rules! for_each_method {
    ($callback:ident) => {
        $callback!(crate::rpc::chain::ChainGetMessage);
        $callback!(crate::rpc::chain::ChainGetParentMessages);
        $callback!(crate::rpc::chain::ChainGetParentReceipts);
        $callback!(crate::rpc::chain::ChainGetMessagesInTipset);
        $callback!(crate::rpc::chain::ChainExport);
        $callback!(crate::rpc::chain::ChainReadObj);
        $callback!(crate::rpc::chain::ChainHasObj);
        $callback!(crate::rpc::chain::ChainGetBlockMessages);
        $callback!(crate::rpc::chain::ChainGetPath);
        $callback!(crate::rpc::chain::ChainGetTipSetByHeight);
        $callback!(crate::rpc::chain::ChainGetTipSetAfterHeight);
        $callback!(crate::rpc::chain::ChainGetGenesis);
        $callback!(crate::rpc::chain::ChainHead);
        $callback!(crate::rpc::chain::ChainGetBlock);
        $callback!(crate::rpc::chain::ChainGetTipSet);
        $callback!(crate::rpc::chain::ChainSetHead);
        $callback!(crate::rpc::chain::ChainGetMinBaseFee);
    };
}
pub(crate) use for_each_method;

pub enum ChainGetMessage {}
impl RpcMethod<1> for ChainGetMessage {
    const NAME: &'static str = "Filecoin.ChainGetMessage";
    const PARAM_NAMES: [&'static str; 1] = ["msg_cid"];
    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Params = (LotusJson<Cid>,);
    type Ok = LotusJson<Message>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (LotusJson(msg_cid),): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let chain_message: ChainMessage = ctx
            .state_manager
            .blockstore()
            .get_cbor(&msg_cid)?
            .with_context(|| format!("can't find message with cid {msg_cid}"))?;
        Ok(LotusJson(match chain_message {
            ChainMessage::Signed(m) => m.into_message(),
            ChainMessage::Unsigned(m) => m,
        }))
    }
}

pub enum ChainGetParentMessages {}
impl RpcMethod<1> for ChainGetParentMessages {
    const NAME: &'static str = "Filecoin.ChainGetParentMessages";
    const PARAM_NAMES: [&'static str; 1] = ["block_cid"];
    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Params = (LotusJson<Cid>,);
    type Ok = LotusJson<Vec<ApiMessage>>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (LotusJson(block_cid),): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let store = ctx.state_manager.blockstore();
        let block_header: CachingBlockHeader = store
            .get_cbor(&block_cid)?
            .with_context(|| format!("can't find block header with cid {block_cid}"))?;
        if block_header.epoch == 0 {
            Ok(LotusJson(vec![]))
        } else {
            let parent_tipset = Tipset::load_required(store, &block_header.parents)?;
            let messages = load_api_messages_from_tipset(store, &parent_tipset)?;
            Ok(LotusJson(messages))
        }
    }
}

pub enum ChainGetParentReceipts {}
impl RpcMethod<1> for ChainGetParentReceipts {
    const NAME: &'static str = "Filecoin.ChainGetParentReceipts";
    const PARAM_NAMES: [&'static str; 1] = ["block_cid"];
    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Params = (LotusJson<Cid>,);
    type Ok = LotusJson<Vec<ApiReceipt>>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (LotusJson(block_cid),): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let store = ctx.state_manager.blockstore();
        let block_header: CachingBlockHeader = store
            .get_cbor(&block_cid)?
            .with_context(|| format!("can't find block header with cid {block_cid}"))?;
        let mut receipts = Vec::new();
        if block_header.epoch == 0 {
            return Ok(LotusJson(vec![]));
        }

        // Try Receipt_v4 first. (Receipt_v4 and Receipt_v3 are identical, use v4 here)
        if let Ok(amt) =
            Amt::<fvm_shared4::receipt::Receipt, _>::load(&block_header.message_receipts, store)
                .map_err(|_| {
                    ErrorObjectOwned::owned::<()>(
                        1,
                        format!(
                            "failed to root: ipld: could not find {}",
                            block_header.message_receipts
                        ),
                        None,
                    )
                })
        {
            amt.for_each(|_, receipt| {
                receipts.push(ApiReceipt {
                    exit_code: receipt.exit_code.into(),
                    return_data: receipt.return_data.clone().into(),
                    gas_used: receipt.gas_used,
                    events_root: receipt.events_root.into(),
                });
                Ok(())
            })?;
        } else {
            // Fallback to Receipt_v2.
            let amt = Amt::<fvm_shared2::receipt::Receipt, _>::load(
                &block_header.message_receipts,
                store,
            )
            .map_err(|_| {
                ErrorObjectOwned::owned::<()>(
                    1,
                    format!(
                        "failed to root: ipld: could not find {}",
                        block_header.message_receipts
                    ),
                    None,
                )
            })?;
            amt.for_each(|_, receipt| {
                receipts.push(ApiReceipt {
                    exit_code: receipt.exit_code.into(),
                    return_data: receipt.return_data.clone().into(),
                    gas_used: receipt.gas_used as _,
                    events_root: None.into(),
                });
                Ok(())
            })?;
        }

        Ok(LotusJson(receipts))
    }
}

pub enum ChainGetMessagesInTipset {}
impl RpcMethod<1> for ChainGetMessagesInTipset {
    const NAME: &'static str = "Filecoin.ChainGetMessagesInTipset";
    const PARAM_NAMES: [&'static str; 1] = ["tsk"];
    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Params = (LotusJson<TipsetKey>,);
    type Ok = LotusJson<Vec<ApiMessage>>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (LotusJson(tsk),): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let store = ctx.chain_store.blockstore();
        let tipset = Tipset::load_required(store, &tsk)?;
        let messages = load_api_messages_from_tipset(store, &tipset)?;
        Ok(LotusJson(messages))
    }
}

pub enum ChainExport {}
impl RpcMethod<1> for ChainExport {
    const NAME: &'static str = "Filecoin.ChainExport";
    const PARAM_NAMES: [&'static str; 1] = ["params"];
    const API_VERSION: ApiVersion = ApiVersion::V0;

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
            tipset_keys: LotusJson(ApiTipsetKey(tsk)),
            skip_checksum,
            dry_run,
        } = params;

        static LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

        let _locked = LOCK.try_lock();
        if _locked.is_err() {
            return Err(anyhow::anyhow!("Another chain export job is still in progress").into());
        }

        let chain_finality = ctx.state_manager.chain_config().policy.chain_finality;
        if recent_roots < chain_finality {
            return Err(anyhow::anyhow!(format!(
                "recent-stateroots must be greater than {chain_finality}"
            ))
            .into());
        }

        let head = ctx.chain_store.load_required_tipset_or_heaviest(&tsk)?;
        let start_ts = ctx.chain_store.chain_index.tipset_by_height(
            epoch,
            head,
            ResolveNullTipset::TakeOlder,
        )?;

        match if dry_run {
            crate::chain::export::<Sha256>(
                Arc::clone(&ctx.chain_store.db),
                &start_ts,
                recent_roots,
                VoidAsyncWriter,
                CidHashSet::default(),
                skip_checksum,
            )
            .await
        } else {
            let file = tokio::fs::File::create(&output_path).await?;
            crate::chain::export::<Sha256>(
                Arc::clone(&ctx.chain_store.db),
                &start_ts,
                recent_roots,
                file,
                CidHashSet::default(),
                skip_checksum,
            )
            .await
        } {
            Ok(checksum_opt) => Ok(checksum_opt.map(|hash| hash.encode_hex())),
            Err(e) => Err(anyhow::anyhow!(e).into()),
        }
    }
}

pub enum ChainReadObj {}
impl RpcMethod<1> for ChainReadObj {
    const NAME: &'static str = "Filecoin.ChainReadObj";
    const PARAM_NAMES: [&'static str; 1] = ["cid"];
    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Params = (LotusJson<Cid>,);
    type Ok = LotusJson<Vec<u8>>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (LotusJson(cid),): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let bytes = ctx
            .state_manager
            .blockstore()
            .get(&cid)?
            .with_context(|| format!("can't find object with cid={cid}"))?;
        Ok(LotusJson(bytes))
    }
}

pub enum ChainHasObj {}
impl RpcMethod<1> for ChainHasObj {
    const NAME: &'static str = "Filecoin.ChainHasObj";
    const PARAM_NAMES: [&'static str; 1] = ["cid"];
    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Params = (LotusJson<Cid>,);
    type Ok = bool;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (LotusJson(cid),): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        Ok(ctx.state_manager.blockstore().get(&cid)?.is_some())
    }
}

pub enum ChainGetBlockMessages {}
impl RpcMethod<1> for ChainGetBlockMessages {
    const NAME: &'static str = "Filecoin.ChainGetBlockMessages";
    const PARAM_NAMES: [&'static str; 1] = ["cid"];
    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Params = (LotusJson<Cid>,);
    type Ok = BlockMessages;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (LotusJson(cid),): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let blk: CachingBlockHeader = ctx
            .state_manager
            .blockstore()
            .get_cbor(&cid)?
            .context("can't find block with that cid")?;
        let blk_msgs = &blk.messages;
        let (unsigned_cids, signed_cids) =
            crate::chain::read_msg_cids(ctx.state_manager.blockstore(), blk_msgs)?;
        let (bls_msg, secp_msg) = crate::chain::block_messages_from_cids(
            ctx.state_manager.blockstore(),
            &unsigned_cids,
            &signed_cids,
        )?;
        let cids = unsigned_cids
            .into_iter()
            .chain(signed_cids)
            .collect::<Vec<_>>()
            .into();

        let ret = BlockMessages {
            bls_msg: bls_msg.into(),
            secp_msg: secp_msg.into(),
            cids,
        };
        Ok(ret)
    }
}

pub enum ChainGetPath {}
impl RpcMethod<2> for ChainGetPath {
    const NAME: &'static str = "Filecoin.ChainGetPath";
    const PARAM_NAMES: [&'static str; 2] = ["from", "to"];
    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Params = (LotusJson<TipsetKey>, LotusJson<TipsetKey>);
    type Ok = LotusJson<Vec<PathChange>>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (LotusJson(from), LotusJson(to)): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        impl_chain_get_path(&ctx.chain_store, &from, &to)
            .map(LotusJson)
            .map_err(Into::into)
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
    const PARAM_NAMES: [&'static str; 2] = ["height", "tsk"];
    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Params = (ChainEpoch, LotusJson<ApiTipsetKey>);
    type Ok = LotusJson<Tipset>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (height, LotusJson(ApiTipsetKey(tsk))): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx
            .state_manager
            .chain_store()
            .load_required_tipset_or_heaviest(&tsk)?;
        let tss = ctx
            .state_manager
            .chain_store()
            .chain_index
            .tipset_by_height(height, ts, ResolveNullTipset::TakeOlder)?;
        Ok((*tss).clone().into())
    }
}

pub enum ChainGetTipSetAfterHeight {}
impl RpcMethod<2> for ChainGetTipSetAfterHeight {
    const NAME: &'static str = "Filecoin.ChainGetTipSetAfterHeight";
    const PARAM_NAMES: [&'static str; 2] = ["height", "tsk"];
    const API_VERSION: ApiVersion = ApiVersion::V1;

    type Params = (ChainEpoch, LotusJson<ApiTipsetKey>);
    type Ok = LotusJson<Tipset>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (height, LotusJson(ApiTipsetKey(tsk))): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx
            .state_manager
            .chain_store()
            .load_required_tipset_or_heaviest(&tsk)?;
        let tss = ctx
            .state_manager
            .chain_store()
            .chain_index
            .tipset_by_height(height, ts, ResolveNullTipset::TakeNewer)?;
        Ok((*tss).clone().into())
    }
}

pub enum ChainGetGenesis {}
impl RpcMethod<0> for ChainGetGenesis {
    const NAME: &'static str = "Filecoin.ChainGetGenesis";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Params = ();
    type Ok = Option<LotusJson<Tipset>>;

    async fn handle(ctx: Ctx<impl Blockstore>, (): Self::Params) -> Result<Self::Ok, ServerError> {
        let genesis = ctx.state_manager.chain_store().genesis_block_header();
        Ok(Some(Tipset::from(genesis).into()))
    }
}

pub enum ChainHead {}
impl RpcMethod<0> for ChainHead {
    const NAME: &'static str = "Filecoin.ChainHead";
    const PARAM_NAMES: [&'static str; 0] = [];
    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Params = ();
    type Ok = LotusJson<Tipset>;

    async fn handle(ctx: Ctx<impl Blockstore>, (): Self::Params) -> Result<Self::Ok, ServerError> {
        let heaviest = ctx.state_manager.chain_store().heaviest_tipset();
        Ok((*heaviest).clone().into())
    }
}

pub enum ChainGetBlock {}
impl RpcMethod<1> for ChainGetBlock {
    const NAME: &'static str = "Filecoin.ChainGetBlock";
    const PARAM_NAMES: [&'static str; 1] = ["cid"];
    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Params = (LotusJson<Cid>,);
    type Ok = LotusJson<CachingBlockHeader>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (LotusJson(cid),): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let blk: CachingBlockHeader = ctx
            .state_manager
            .blockstore()
            .get_cbor(&cid)?
            .context("can't find BlockHeader with that cid")?;
        Ok(blk.into())
    }
}

pub enum ChainGetTipSet {}
impl RpcMethod<1> for ChainGetTipSet {
    const NAME: &'static str = "Filecoin.ChainGetTipSet";
    const PARAM_NAMES: [&'static str; 1] = ["tsk"];
    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Params = (LotusJson<ApiTipsetKey>,);
    type Ok = LotusJson<Tipset>;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (LotusJson(ApiTipsetKey(tsk)),): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let ts = ctx
            .state_manager
            .chain_store()
            .load_required_tipset_or_heaviest(&tsk)?;
        Ok((*ts).clone().into())
    }
}

pub enum ChainSetHead {}
impl RpcMethod<1> for ChainSetHead {
    const NAME: &'static str = "Filecoin.ChainSetHead";
    const PARAM_NAMES: [&'static str; 1] = ["tsk"];
    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Params = (LotusJson<ApiTipsetKey>,);
    type Ok = ();

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (LotusJson(ApiTipsetKey(tsk)),): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        // This is basically a port of the reference implementation at
        // https://github.com/filecoin-project/lotus/blob/v1.23.0/node/impl/full/chain.go#L321

        let new_head = ctx
            .state_manager
            .chain_store()
            .load_required_tipset_or_heaviest(&tsk)?;
        let mut current = ctx.state_manager.chain_store().heaviest_tipset();
        while current.epoch() >= new_head.epoch() {
            for cid in current.key().to_cids() {
                ctx.state_manager
                    .chain_store()
                    .unmark_block_as_validated(&cid);
            }
            let parents = &current.block_headers().first().parents;
            current = ctx
                .state_manager
                .chain_store()
                .chain_index
                .load_required_tipset(parents)?;
        }
        ctx.state_manager
            .chain_store()
            .set_heaviest_tipset(new_head)
            .map_err(Into::into)
    }
}

pub enum ChainGetMinBaseFee {}
impl RpcMethod<1> for ChainGetMinBaseFee {
    const NAME: &'static str = "Filecoin.ChainGetMinBaseFee";
    const PARAM_NAMES: [&'static str; 1] = ["lookback"];
    const API_VERSION: ApiVersion = ApiVersion::V0;

    type Params = (u32,);
    type Ok = String;

    async fn handle(
        ctx: Ctx<impl Blockstore>,
        (lookback,): Self::Params,
    ) -> Result<Self::Ok, ServerError> {
        let mut current = ctx.state_manager.chain_store().heaviest_tipset();
        let mut min_base_fee = current.block_headers().first().parent_base_fee.clone();

        for _ in 0..lookback {
            let parents = &current.block_headers().first().parents;
            current = ctx
                .state_manager
                .chain_store()
                .chain_index
                .load_required_tipset(parents)?;

            min_base_fee =
                min_base_fee.min(current.block_headers().first().parent_base_fee.to_owned());
        }

        Ok(min_base_fee.atto().to_string())
    }
}

pub const CHAIN_NOTIFY: &str = "Filecoin.ChainNotify";
pub(crate) fn chain_notify<DB: Blockstore>(
    _params: Params<'_>,
    data: &crate::rpc::RPCState<DB>,
) -> Subscriber<Vec<ApiHeadChange>> {
    let (sender, receiver) = broadcast::channel(100);

    // As soon as the channel is created, send the current tipset
    let current = data.chain_store.heaviest_tipset();
    let (change, headers) = ("current".into(), current.block_headers().clone().into());
    sender
        .send(vec![ApiHeadChange { change, headers }])
        .expect("receiver is not dropped");

    let mut subscriber = data.chain_store.publisher().subscribe();

    tokio::spawn(async move {
        while let Ok(v) = subscriber.recv().await {
            let (change, headers) = match v {
                HeadChange::Apply(ts) => ("apply".into(), ts.block_headers().clone().into()),
            };

            if sender
                .send(vec![ApiHeadChange { change, headers }])
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
    for block in blocks {
        for msg in block.bls_msgs() {
            let cid = msg.cid()?;
            if seen.insert(cid) {
                messages.push(ApiMessage::new(cid, msg.clone()));
            }
        }

        for msg in block.secp_msgs() {
            let cid = msg.cid()?;
            if seen.insert(cid) {
                messages.push(ApiMessage::new(cid, msg.message.clone()));
            }
        }
    }

    Ok(messages)
}

#[derive(Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BlockMessages {
    #[serde(rename = "BlsMessages")]
    pub bls_msg: LotusJson<Vec<Message>>,
    #[serde(rename = "SecpkMessages")]
    pub secp_msg: LotusJson<Vec<SignedMessage>>,
    #[serde(rename = "Cids")]
    pub cids: LotusJson<Vec<Cid>>,
}

lotus_json_with_self!(BlockMessages);

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct ApiReceipt {
    // Exit status of message execution
    pub exit_code: ExitCode,
    // `Return` value if the exit code is zero
    #[serde(rename = "Return")]
    pub return_data: LotusJson<RawBytes>,
    // Non-negative value of GasUsed
    pub gas_used: u64,
    pub events_root: LotusJson<Option<Cid>>,
}

lotus_json_with_self!(ApiReceipt);

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct ApiMessage {
    cid: Cid,
    message: Message,
}

impl ApiMessage {
    pub fn new(cid: Cid, message: Message) -> Self {
        Self { cid, message }
    }
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct ApiMessageLotusJson {
    cid: LotusJson<Cid>,
    message: LotusJson<Message>,
}

impl HasLotusJson for ApiMessage {
    type LotusJson = ApiMessageLotusJson;
    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        vec![]
    }
    fn into_lotus_json(self) -> Self::LotusJson {
        ApiMessageLotusJson {
            cid: LotusJson(self.cid),
            message: LotusJson(self.message),
        }
    }
    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        ApiMessage {
            cid: lotus_json.cid.into_inner(),
            message: lotus_json.message.into_inner(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChainExportParams {
    pub epoch: ChainEpoch,
    pub recent_roots: i64,
    pub output_path: PathBuf,
    pub tipset_keys: LotusJson<ApiTipsetKey>,
    pub skip_checksum: bool,
    pub dry_run: bool,
}

lotus_json_with_self!(ChainExportParams);

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct ApiHeadChange {
    #[serde(rename = "Type")]
    pub change: String,
    #[serde(rename = "Val", with = "crate::lotus_json")]
    pub headers: Vec<CachingBlockHeader>,
}

lotus_json_with_self!(ApiHeadChange);

#[derive(PartialEq, Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PathChange<T = Arc<Tipset>> {
    Revert(T),
    Apply(T),
}
impl HasLotusJson for PathChange {
    type LotusJson = PathChange<LotusJson<Tipset>>;

    #[cfg(test)]
    fn snapshots() -> Vec<(serde_json::Value, Self)> {
        use serde_json::json;
        vec![(
            json!({
                "revert": {
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
            PathChange::Revert(it) => PathChange::Revert(LotusJson(Tipset::clone(&it))),
            PathChange::Apply(it) => PathChange::Apply(LotusJson(Tipset::clone(&it))),
        }
    }

    fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
        match lotus_json {
            PathChange::Revert(it) => PathChange::Revert(it.into_inner().into()),
            PathChange::Apply(it) => PathChange::Apply(it.into_inner().into()),
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

pub async fn get_parent_receipts<DB: Blockstore + Send + Sync + 'static>(
    data: Ctx<DB>,
    message_receipts: Cid,
) -> Result<Vec<ApiReceipt>> {
    let store = data.state_manager.blockstore();

    let mut receipts = Vec::new();

    // Try Receipt_v4 first. (Receipt_v4 and Receipt_v3 are identical, use v4 here)
    if let Ok(amt) = Amt::<fvm_shared4::receipt::Receipt, _>::load(&message_receipts, store)
        .map_err(|_| {
            ErrorObjectOwned::owned::<()>(
                1,
                format!("failed to root: ipld: could not find {}", message_receipts),
                None,
            )
        })
    {
        amt.for_each(|_, receipt| {
            receipts.push(ApiReceipt {
                exit_code: receipt.exit_code.into(),
                return_data: LotusJson(receipt.return_data.clone()),
                gas_used: receipt.gas_used,
                events_root: LotusJson(receipt.events_root),
            });
            Ok(())
        })?;
    } else {
        // Fallback to Receipt_v2.
        let amt = Amt::<fvm_shared2::receipt::Receipt, _>::load(&message_receipts, store).map_err(
            |_| {
                ErrorObjectOwned::owned::<()>(
                    1,
                    format!("failed to root: ipld: could not find {}", message_receipts),
                    None,
                )
            },
        )?;
        amt.for_each(|_, receipt| {
            receipts.push(ApiReceipt {
                exit_code: receipt.exit_code.into(),
                return_data: LotusJson(receipt.return_data.clone()),
                gas_used: receipt.gas_used as _,
                events_root: LotusJson(None),
            });
            Ok(())
        })?;
    }

    Ok(receipts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use PathChange::{Apply, Revert};

    use crate::{
        blocks::{chain4u, Chain4U, RawBlockHeader},
        db::{car::PlainCar, MemoryDB},
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
