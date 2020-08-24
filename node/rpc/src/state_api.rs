// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::RpcState;
use actor::miner::{
    compute_proving_period_deadline, ChainSectorInfo, DeadlineInfo, Deadlines, Fault, MinerInfo,
    SectorOnChainInfo, SectorPreCommitOnChainInfo, State,
};
use address::Address;
use async_std::task;
use bitfield::json::BitFieldJson;
use blocks::{tipset_json::TipsetJson, Tipset, TipsetKeys};
use blockstore::BlockStore;
use cid::{json::CidJson, Cid};
use clock::ChainEpoch;
use fil_types::SectorNumber;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use message::{
    message_receipt::json::MessageReceiptJson,
    unsigned_message::{json::UnsignedMessageJson, UnsignedMessage},
};
use serde::{Deserialize, Serialize};
use state_manager::{InvocResult, MarketBalance, StateManager};
use state_tree::StateTree;
use wallet::KeyStore;

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MessageLookup {
    pub receipt: MessageReceiptJson,
    pub tipset: TipsetJson,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InvocResultJson {
    pub msg: UnsignedMessageJson,
    pub msg_rct: Option<MessageReceiptJson>,
    pub error: Option<String>,
}

impl From<InvocResult> for InvocResultJson {
    fn from(invoc: InvocResult) -> Self {
        InvocResultJson {
            msg: invoc.msg.into(),
            msg_rct: invoc.msg_rct.map(|s| s.into()),
            error: invoc.error,
        }
    }
}

/// returns info about the given miner's sectors. If the filter bitfield is nil, all sectors are included.
/// If the filterOut boolean is set to true, any sectors in the filter are excluded.
/// If false, only those sectors in the filter are included.
pub(crate) async fn state_miner_sector<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(Address, BitFieldJson, bool, TipsetKeys)>,
) -> Result<Vec<ChainSectorInfo>, JsonRpcError> {
    let (address, filter, filter_out, key) = params;
    let mut bitfield_filter = filter.into();
    let state_manager = &data.state_manager;
    let tipset = chain::tipset_from_keys(data.state_manager.get_block_store_ref(), &key)?;
    let mut filter = Some(&mut bitfield_filter);
    state_manager::utils::get_miner_sector_set(
        &state_manager,
        &tipset,
        &address,
        &mut filter,
        filter_out,
    )
    .map_err(|e| e.into())
}

/// runs the given message and returns its result without any persisted changes.
pub(crate) async fn state_call<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(UnsignedMessageJson, TipsetKeys)>,
) -> Result<InvocResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (unsigned_msg_json, key) = params;
    let mut message: UnsignedMessage = unsigned_msg_json.into();
    let tipset = chain::tipset_from_keys(data.state_manager.get_block_store_ref(), &key)?;
    state_manager
        .call(&mut message, Some(tipset))
        .map_err(|e| e.into())
}

/// returns all the proving deadlines for the given miner
pub(crate) async fn state_miner_deadlines<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(Address, TipsetKeys)>,
) -> Result<Deadlines, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (actor, key) = params;
    let tipset = chain::tipset_from_keys(data.state_manager.get_block_store_ref(), &key)?;
    state_manager::utils::get_miner_deadlines(&state_manager, &tipset, &actor).map_err(|e| e.into())
}

/// returns the PreCommit info for the specified miner's sector
pub(crate) async fn state_sector_precommit_info<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(Address, SectorNumber, TipsetKeys)>,
) -> Result<SectorPreCommitOnChainInfo, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (address, sector_number, key) = params;
    let tipset = chain::tipset_from_keys(data.state_manager.get_block_store_ref(), &key)?;
    state_manager::utils::precommit_info(&state_manager, &address, &sector_number, &tipset)
        .map_err(|e| e.into())
}

/// returns info about those sectors that a given miner is actively proving.
pub(crate) async fn state_miner_proving_set<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(Address, TipsetKeys)>,
) -> Result<Vec<SectorOnChainInfo>, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (address, key) = params;
    let tipset = chain::tipset_from_keys(data.state_manager.get_block_store_ref(), &key)?;
    let miner_actor_state: State =
        state_manager.load_actor_state(&address, &tipset.parent_state())?;
    state_manager::utils::get_proving_set_raw(&state_manager, &miner_actor_state)
        .map_err(|e| e.into())
}

/// StateMinerInfo returns info about the indicated miner
pub async fn state_miner_info<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(Address, TipsetKeys)>,
) -> Result<MinerInfo, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (actor, key) = params;
    let tipset = chain::tipset_from_keys(state_manager.get_block_store_ref(), &key)?;
    state_manager::utils::get_miner_info(&state_manager, &tipset, &actor).map_err(|e| e.into())
}

/// returns the on-chain info for the specified miner's sector
pub async fn state_sector_info<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(Address, SectorNumber, TipsetKeys)>,
) -> Result<Option<SectorOnChainInfo>, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (address, sector_number, key) = params;
    let tipset = chain::tipset_from_keys(data.state_manager.get_block_store_ref(), &key)?;
    state_manager::utils::miner_sector_info(&state_manager, &address, &sector_number, &tipset)
        .map_err(|e| e.into())
}

/// calculates the deadline at some epoch for a proving period
/// and returns the deadline-related calculations.
pub(crate) async fn state_miner_proving_deadline<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(Address, TipsetKeys)>,
) -> Result<DeadlineInfo, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (actor, key) = params;
    let tipset = chain::tipset_from_keys(data.state_manager.get_block_store_ref(), &key)?;
    let miner_actor_state: State =
        state_manager.load_actor_state(&actor, &tipset.parent_state())?;
    Ok(compute_proving_period_deadline(
        miner_actor_state.proving_period_start,
        tipset.epoch(),
    ))
}

/// returns a single non-expired Faults that occur within lookback epochs of the given tipset
pub(crate) async fn state_miner_faults<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(Address, TipsetKeys)>,
) -> Result<BitFieldJson, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (actor, key) = params;
    let tipset = chain::tipset_from_keys(data.state_manager.get_block_store_ref(), &key)?;
    state_manager::utils::get_miner_faults(&state_manager, &tipset, &actor)
        .map(|s| s.into())
        .map_err(|e| e.into())
}

/// returns all non-expired Faults that occur within lookback epochs of the given tipset
pub(crate) async fn state_all_miner_faults<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(ChainEpoch, TipsetKeys)>,
) -> Result<Vec<Fault>, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (look_back, end_tsk) = params;
    let tipset = chain::tipset_from_keys(data.state_manager.get_block_store_ref(), &end_tsk)?;
    let cut_off = tipset.epoch() - look_back;
    let miners = state_manager::utils::list_miner_actors(&state_manager, &tipset)?;
    let mut all_faults = Vec::new();
    miners
        .iter()
        .map(|m| {
            let miner_actor_state: State = state_manager
                .load_actor_state(&m, &tipset.parent_state())
                .map_err(|e| e.to_string())?;
            let block_store = state_manager.get_block_store_ref();
            miner_actor_state.for_each_fault_epoch(block_store, |fault_start: i64, _| {
                if fault_start >= cut_off {
                    all_faults.push(Fault {
                        miner: *m,
                        fault: fault_start,
                    })
                }
                Ok(())
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(all_faults)
}
/// returns a bitfield indicating the recovering sectors of the given miner
pub(crate) async fn state_miner_recoveries<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(Address, TipsetKeys)>,
) -> Result<BitFieldJson, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (actor, key) = params;
    let tipset = chain::tipset_from_keys(data.state_manager.get_block_store_ref(), &key)?;
    state_manager::utils::get_miner_recoveries(&state_manager, &tipset, &actor)
        .map(|s| s.into())
        .map_err(|e| e.into())
}

/// returns the result of executing the indicated message, assuming it was executed in the indicated tipset.
pub(crate) async fn state_replay<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(CidJson, TipsetKeys)>,
) -> Result<InvocResultJson, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (cidjson, key) = params;
    let cid = cidjson.into();
    let tipset = chain::tipset_from_keys(data.state_manager.get_block_store_ref(), &key)?;
    let (msg, ret) = state_manager.replay(&tipset, &cid)?;

    Ok(InvocResultJson {
        msg: msg.into(),
        msg_rct: ret.as_ref().map(|s| s.msg_receipt.clone().into()),
        error: ret
            .map(|act| act.act_error.map(|e| e.to_string()))
            .unwrap_or_default(),
    })
}

/// returns the indicated actor's nonce and balance.
pub(crate) async fn state_get_actor<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(Address, TipsetKeys)>,
) -> Result<Option<actor::ActorState>, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (actor, key) = params;
    let tipset = chain::tipset_from_keys(data.state_manager.get_block_store_ref(), &key)?;
    let state = state_for_ts(&state_manager, Some(tipset))?;
    state.get_actor(&actor).map_err(|e| e.into())
}

/// returns the public key address of the given ID address
pub(crate) async fn state_account_key<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(Address, TipsetKeys)>,
) -> Result<Option<Address>, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (actor, key) = params;
    let tipset = chain::tipset_from_keys(data.state_manager.get_block_store_ref(), &key)?;
    let state = state_for_ts(&state_manager, Some(tipset))?;
    let address =
        interpreter::resolve_to_key_addr(&state, state_manager.get_block_store_ref(), &actor)?;
    Ok(address.into())
}
/// retrieves the ID address of the given address
pub(crate) async fn state_lookup_id<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(Address, TipsetKeys)>,
) -> Result<Option<Address>, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (address, key) = params;
    let tipset = chain::tipset_from_keys(data.state_manager.get_block_store_ref(), &key)?;
    let state = state_for_ts(&state_manager, Some(tipset))?;
    state.lookup_id(&address).map_err(|e| e.into())
}

/// looks up the Escrow and Locked balances of the given address in the Storage Market
pub(crate) async fn state_market_balance<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(Address, TipsetKeys)>,
) -> Result<MarketBalance, JsonRpcError> {
    let (address, key) = params;
    let tipset = chain::tipset_from_keys(data.state_manager.get_block_store_ref(), &key)?;
    data.state_manager
        .market_balance(&address, &tipset)
        .map_err(|e| e.into())
}

/// returns the message receipt for the given message
pub(crate) async fn state_get_receipt<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(CidJson, TipsetKeys)>,
) -> Result<MessageReceiptJson, JsonRpcError> {
    let (cidjson, key) = params;
    let state_manager = &data.state_manager;
    let cid = cidjson.into();
    let tipset = chain::tipset_from_keys(data.state_manager.get_block_store_ref(), &key)?;
    state_manager
        .get_receipt(&tipset, &cid)
        .map(|s| s.into())
        .map_err(|e| e.into())
}
/// looks back in the chain for a message. If not found, it blocks until the
/// message arrives on chain, and gets to the indicated confidence depth.
pub(crate) async fn state_wait_msg<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(CidJson, i64)>,
) -> Result<MessageLookup, JsonRpcError> {
    let (cidjson, confidence) = params;
    let state_manager = &data.state_manager;
    let cid: Cid = cidjson.into();
    let (tipset, receipt) = StateManager::wait_for_message(
        state_manager.get_block_store(),
        state_manager.get_subscriber(),
        &cid,
        confidence,
    )
    .await?;
    let tipset = tipset.ok_or_else(|| "wait for msg returned empty tuple")?;
    let receipt = receipt.ok_or_else(|| "wait for msg returned empty receipt")?;
    let tipset: &Tipset = &*tipset;
    let tipset_json: TipsetJson = tipset.clone().into();
    Ok(MessageLookup {
        receipt: receipt.into(),
        tipset: tipset_json,
    })
}

/// returns a state tree given a tipset
pub fn state_for_ts<DB>(
    state_manager: &StateManager<DB>,
    maybe_tipset: Option<Tipset>,
) -> Result<StateTree<DB>, JsonRpcError>
where
    DB: BlockStore,
{
    let block_store = state_manager.get_block_store_ref();
    let maybe_tipset = if maybe_tipset.is_none() {
        chain::get_heaviest_tipset(block_store)?
    } else {
        maybe_tipset
    };

    let tipset = maybe_tipset.ok_or_else(|| {
        Box::new(chain::Error::Other(
            "Could not get heaviest tipset".to_string(),
        ))
    })?;
    let (st, _) = task::block_on(state_manager.tipset_state(&tipset))?;
    let state_tree = StateTree::new_from_root(block_store, &st)?;
    Ok(state_tree)
}
