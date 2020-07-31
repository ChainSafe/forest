// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::miner::{
    compute_proving_period_deadline, ChainSectorInfo, DeadlineInfo, Deadlines, Fault, MinerInfo,
    SectorOnChainInfo, SectorPreCommitOnChainInfo, State,
};
use address::Address;
use async_std::sync::Arc;
use async_std::task;
use bitfield::BitField;
use blocks::{Tipset, TipsetKeys};
use blockstore::BlockStore;
use chain::ChainStore;
use cid::Cid;
use clock::ChainEpoch;
use fil_types::SectorNumber;
use message::{MessageReceipt, UnsignedMessage};
use num_bigint::BigUint;
use num_traits::identities::Zero;
use state_manager::{InvocResult, MarketBalance, StateManager};
use state_tree::StateTree;
use std::error::Error;

type BoxError = Box<dyn Error + 'static>;
pub struct MessageLookup {
    pub receipt: MessageReceipt,
    pub tipset: Arc<Tipset>,
}
pub fn state_get_network_name<DB>(state_manager: &StateManager<DB>) -> Result<String, BoxError>
where
    DB: BlockStore,
{
    let maybe_heaviest_tipset: Option<Tipset> =
        chain::get_heaviest_tipset(state_manager.get_block_store_ref())?;
    let heaviest_tipset: Tipset = maybe_heaviest_tipset.unwrap();
    state_manager
        .get_network_name(heaviest_tipset.parent_state())
        .map_err(|e| e.into())
}

/// returns info about the given miner's sectors. If the filter bitfield is nil, all sectors are included.
/// If the filterOut boolean is set to true, any sectors in the filter are excluded.
/// If false, only those sectors in the filter are included.
pub fn state_miner_sectors<DB>(
    state_manager: &StateManager<DB>,
    address: &Address,
    filter: &mut BitField,
    filter_out: bool,
    key: &TipsetKeys,
) -> Result<Vec<ChainSectorInfo>, BoxError>
where
    DB: BlockStore,
{
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    let mut filter = Some(filter);
    state_manager::utils::get_miner_sector_set(
        &state_manager,
        &tipset,
        address,
        &mut filter,
        filter_out,
    )
    .map_err(|e| e.into())
}

/// returns info about those sectors that a given miner is actively proving.
pub fn state_miner_proving_set<DB>(
    state_manager: &StateManager<DB>,
    address: &Address,
    key: &TipsetKeys,
) -> Result<Vec<SectorOnChainInfo>, BoxError>
where
    DB: BlockStore,
{
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    let miner_actor_state: State =
        state_manager.load_actor_state(&address, &tipset.parent_state())?;
    state_manager::utils::get_proving_set_raw(&state_manager, &miner_actor_state)
        .map_err(|e| e.into())
}

/// StateMinerInfo returns info about the indicated miner
pub fn state_miner_info<DB>(
    state_manager: &StateManager<DB>,
    actor: &Address,
    key: &TipsetKeys,
) -> Result<MinerInfo, BoxError>
where
    DB: BlockStore,
{
    let tipset = chain::tipset_from_keys(state_manager.get_block_store_ref(), key)?;
    state_manager::utils::get_miner_info(state_manager, &tipset, actor).map_err(|e| e.into())
}

/// returns the on-chain info for the specified miner's sector
pub fn state_sector_info<DB>(
    state_manager: &StateManager<DB>,
    address: &Address,
    sector_number: &SectorNumber,
    key: &TipsetKeys,
) -> Result<Option<SectorOnChainInfo>, BoxError>
where
    DB: BlockStore,
{
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    state_manager::utils::miner_sector_info(&state_manager, address, sector_number, &tipset)
        .map_err(|e| e.into())
}

/// returns the PreCommit info for the specified miner's sector
pub fn state_sector_precommit_info<DB>(
    state_manager: &StateManager<DB>,
    address: &Address,
    sector_number: &SectorNumber,
    key: &TipsetKeys,
) -> Result<SectorPreCommitOnChainInfo, BoxError>
where
    DB: BlockStore,
{
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    state_manager::utils::precommit_info(&state_manager, address, sector_number, &tipset)
        .map_err(|e| e.into())
}

/// returns all the proving deadlines for the given miner
pub fn state_miner_deadlines<DB>(
    state_manager: &StateManager<DB>,
    actor: &Address,
    key: &TipsetKeys,
) -> Result<Deadlines, BoxError>
where
    DB: BlockStore,
{
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    state_manager::utils::get_miner_deadlines(&state_manager, &tipset, actor).map_err(|e| e.into())
}

/// calculates the deadline at some epoch for a proving period
/// and returns the deadline-related calculations.
pub fn state_miner_proving_deadline<DB>(
    state_manager: &StateManager<DB>,
    actor: &Address,
    key: &TipsetKeys,
) -> Result<DeadlineInfo, BoxError>
where
    DB: BlockStore,
{
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    let miner_actor_state: State =
        state_manager.load_actor_state(&actor, &tipset.parent_state())?;
    Ok(compute_proving_period_deadline(
        miner_actor_state.proving_period_start,
        tipset.epoch(),
    ))
}

/// returns a single non-expired Faults that occur within lookback epochs of the given tipset
pub fn state_miner_faults<DB>(
    state_manager: &StateManager<DB>,
    actor: &Address,
    key: &TipsetKeys,
) -> Result<BitField, BoxError>
where
    DB: BlockStore,
{
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    state_manager::utils::get_miner_faults(&state_manager, &tipset, actor).map_err(|e| e.into())
}

/// returns all non-expired Faults that occur within lookback epochs of the given tipset
pub fn state_all_miner_faults<DB>(
    state_manager: &StateManager<DB>,
    look_back: ChainEpoch,
    end_tsk: &TipsetKeys,
) -> Result<Vec<Fault>, BoxError>
where
    DB: BlockStore,
{
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(end_tsk)?;
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
            miner_actor_state.for_each_fault_epoch(
                block_store,
                |fault_start: i64, _| -> Result<(), String> {
                    if fault_start >= cut_off {
                        all_faults.push(Fault {
                            miner: *m,
                            fault: fault_start,
                        })
                    }
                    Ok(())
                },
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(all_faults)
}
/// returns a bitfield indicating the recovering sectors of the given miner
pub fn state_miner_recoveries<DB>(
    state_manager: &StateManager<DB>,
    actor: &Address,
    key: &TipsetKeys,
) -> Result<BitField, BoxError>
where
    DB: BlockStore,
{
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    state_manager::utils::get_miner_recoveries(&state_manager, &tipset, actor).map_err(|e| e.into())
}

pub fn state_pledge_collateral<DB>(
    _state_manager: &StateManager<DB>,
    _: &TipsetKeys,
) -> Result<BigUint, BoxError>
where
    DB: BlockStore,
{
    Ok(BigUint::zero())
}

/// runs the given message and returns its result without any persisted changes.
pub fn state_call<DB>(
    state_manager: &StateManager<DB>,
    message: &mut UnsignedMessage,
    key: &TipsetKeys,
) -> Result<InvocResult<UnsignedMessage>, BoxError>
where
    DB: BlockStore,
{
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    state_manager
        .call(message, Some(tipset))
        .map_err(|e| e.into())
}

/// returns the result of executing the indicated message, assuming it was executed in the indicated tipset.
pub fn state_reply<DB>(
    state_manager: &StateManager<DB>,
    key: &TipsetKeys,
    cid: &Cid,
) -> Result<InvocResult<UnsignedMessage>, BoxError>
where
    DB: BlockStore,
{
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    let (msg, ret) = state_manager.replay(&tipset, cid)?;

    Ok(InvocResult {
        msg,
        msg_rct: ret.as_ref().map(|s| s.msg_receipt.clone()),
        actor_error: ret
            .map(|act| act.act_error.map(|e| e.to_string()))
            .unwrap_or_default(),
    })
}

/// returns a state tree given a tipset
pub fn state_for_ts<DB>(
    state_manager: &StateManager<DB>,
    maybe_tipset: Option<Tipset>,
) -> Result<StateTree<DB>, BoxError>
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

/// returns the indicated actor's nonce and balance.
pub fn state_get_actor<DB>(
    state_manager: &StateManager<DB>,
    actor: &Address,
    key: &TipsetKeys,
) -> Result<Option<actor::ActorState>, BoxError>
where
    DB: BlockStore,
{
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    let state = state_for_ts(state_manager, Some(tipset))?;
    state.get_actor(actor).map_err(|e| e.into())
}

/// returns the public key address of the given ID address
pub fn state_account_key<DB>(
    state_manager: &StateManager<DB>,
    actor: &Address,
    key: &TipsetKeys,
) -> Result<Address, BoxError>
where
    DB: BlockStore,
{
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    let state = state_for_ts(state_manager, Some(tipset))?;
    let address =
        interpreter::resolve_to_key_addr(&state, state_manager.get_block_store_ref(), actor)?;
    Ok(address)
}

/// retrieves the ID address of the given address
pub fn state_lookup_id<DB>(
    state_manager: &StateManager<DB>,
    address: &Address,
    key: &TipsetKeys,
) -> Result<Option<Address>, BoxError>
where
    DB: BlockStore,
{
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    let state = state_for_ts(state_manager, Some(tipset))?;
    state.lookup_id(address).map_err(|e| e.into())
}

/// looks up the Escrow and Locked balances of the given address in the Storage Market
pub fn state_market_balance<DB>(
    state_manager: &mut StateManager<DB>,
    address: &Address,
    key: &TipsetKeys,
) -> Result<MarketBalance, BoxError>
where
    DB: BlockStore,
{
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    state_manager
        .market_balance(address, &tipset)
        .map_err(|e| e.into())
}

/// returns the message receipt for the given message
pub fn state_get_receipt<DB>(
    state_manager: &StateManager<DB>,
    msg: &Cid,
    key: &TipsetKeys,
) -> Result<MessageReceipt, BoxError>
where
    DB: BlockStore,
{
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    state_manager
        .get_receipt(&tipset, msg)
        .map_err(|e| e.into())
}

/// looks back in the chain for a message. If not found, it blocks until the
/// message arrives on chain, and gets to the indicated confidence depth.
pub fn state_wait_msg<DB: BlockStore + Send + Sync + 'static>(
    state_manager: &StateManager<DB>,
    cid: &Cid,
    confidence: i64,
) -> Result<MessageLookup, BoxError> {
    let block_store = state_manager.get_block_store();
    let subscriber = state_manager.get_subscriber();
    let (tipset, receipt) = task::block_on(StateManager::wait_for_message(
        block_store,
        subscriber,
        cid,
        confidence,
    ))?;
    let tipset = tipset.ok_or_else(|| "wait _for_msg returned empty tipset")?;
    let receipt = receipt.ok_or_else(|| "wait_for_msg returned empty message receipt")?;
    Ok(MessageLookup { receipt, tipset })
}
