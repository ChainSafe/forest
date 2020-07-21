
use actor::{
    miner::{
        compute_proving_period_deadline, ChainSectorInfo, DeadlineInfo, Deadlines, Fault,
        MinerInfo, SectorOnChainInfo, SectorPreCommitOnChainInfoJson, State,
    },
    power::Claim,
};
use address::{json::AddressJson};
use async_std::sync::Arc;
use async_std::task;
use bitfield::BitField;
use blocks::{Tipset, TipsetKeys};
use blockstore::BlockStore;
use chain::ChainStore;
use cid::CidJson;
use clock::ChainEpoch;
use fil_types::SectorNumber;
use message::{MessageReceipt, json::UnsignedMessageJson};
use num_bigint::BigUint;
use num_traits::identities::Zero;
use state_manager::{InvocResult, MarketBalance, StateManager};
use state_tree::StateTree;
use std::error::Error;
use jsonrpc_v2::{Data, JsonRpcError, Params};
use crate::RpcState;
use wallet::KeyStore;
use state_manager::InvocResult;

#[derive(Serialize)]
pub struct MessageLookup {
    pub receipt: MessageReceipt,
    pub tipset: Arc<Tipset>,
}

/// returns info about the given miner's sectors. If the filter bitfield is nil, all sectors are included.
/// If the filterOut boolean is set to true, any sectors in the filter are excluded.
/// If false, only those sectors in the filter are included.
pub(crate) async fn state_miner_sector<DB: BlockStore + Send + Sync + 'static,  KS: KeyStore + Send + Sync + 'static>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(AddressJson,BitFieldJson,bool,TipsetKeys)>,
) -> Result<Vec<ChainSectorInfoJson>, JsonRpcError> {
    let state_manager = StateManager::new(data.store);
    let (address_json,bitfield_json,filter,key) = params;
    let address = address_json.into();
    let bitfield = bitfield_json.into();
    let state_manager = StateManager::new(data.store);
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    let mut filter = Some(filter);
    state_manager::utils::get_miner_sector_set(
        &state_manager,
        &tipset,
        address,
        &mut filter,
        filter_out,
    )
    .map(|s|s.into())
    .map_err(|e| e.into())
}

/// runs the given message and returns its result without any persisted changes.
pub(crate) async fn state_call<DB: BlockStore + Send + Sync + 'static,  KS: KeyStore + Send + Sync + 'static>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(UnisignedMessageJson,TipsetKeys)>,
) -> Result<InvocResult<UnsignedMessageJson>, JsonRpcError> {
    let state_manager = StateManager::new(data.store);
    let (unsigned_msg_json,key) = params;
    let unsigned_msg = unsigned_msg_json.into();
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    state_manager
        .call(message, Some(tipset))
        .map(|s|s.into())
        .map_err(|e| e.into())
}

/// returns all the proving deadlines for the given miner
pub(crate) async fn state_miner_deadlines<DB: BlockStore + Send + Sync + 'static,  KS: KeyStore + Send + Sync + 'static>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(AddressJson,TipsetKeys)>,
) -> Result<Deadlines, JsonRpcError> {
    let state_manager = StateManager::new(data.store);
    let (address_json,key) = params;
    let address = address_json.into();
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    state_manager::utils::get_miner_deadlines(&state_manager, &tipset, actor).map_err(|e| e.into())
}

/// returns the PreCommit info for the specified miner's sector
pub(crate) async fn state_sector_precommit_info<DB: BlockStore + Send + Sync + 'static,  KS: KeyStore + Send + Sync + 'static>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(AddressJson,SectorNumber,TipsetKeys)>,
) -> Result<SectorPreCommitOnChainInfoJson, JsonRpcError> {
    let state_manager = StateManager::new(data.store);
    let (address_json,sector_number,key) = params;
    let address = address_json.into();
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    state_manager::utils::precommit_info(&state_manager, address, sector_number, &tipset)
        .map(|s|s.into())
        .map_err(|e| e.into())
}

/// returns info about those sectors that a given miner is actively proving.
pub (crate) async fn state_miner_proving_set<DB: BlockStore + Send + Sync + 'static,  KS: KeyStore + Send + Sync + 'static>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(AddressJson,TipsetKeys)>,
) -> Result<Vec<SectorOnChainInfo>, JsonRpcError>
{
    let state_manager = StateManager::new(data.store);
    let (address_json,key) = params;
    let address = address_json.into();
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    let miner_actor_state: State =
        state_manager.load_actor_state(&address, &tipset.parent_state())?;
    state_manager::utils::get_proving_set_raw(&state_manager, &miner_actor_state)
         .map(|s|s.into())
        .map_err(|e| e.into())
}

/// StateMinerInfo returns info about the indicated miner
pub async fn state_miner_info<DB: BlockStore + Send + Sync + 'static,  KS: KeyStore + Send + Sync + 'static>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(AddressJson,TipsetKeys)>,
) -> Result<MinerInfo, JsonRpcError>
{
    let state_manager = StateManager::new(data.store);
    let (address_json,key) = params;
    let address = address_json.into();
    let tipset = chain::tipset_from_keys(state_manager.get_block_store_ref(), key)?;
    state_manager::utils::get_miner_info(&state_manager, &tipset, actor)
    .map(|s|s.into())
    .map_err(|e| e.into())
}

/// returns the on-chain info for the specified miner's sector
pub async fn state_sector_info<DB: BlockStore + Send + Sync + 'static,  KS: KeyStore + Send + Sync + 'static>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(AddressJson,SectorNumber,TipsetKeys)>
) -> Result<Option<SectorOnChainInfo>, JsonRpcError>
{
    let state_manager = StateManager::new(data.store);
    let (address_json,sector_number,key) = params;
    let address = address_json.into();
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    state_manager::utils::miner_sector_info(&state_manager, address, sector_number, &tipset)
    .map(|s|s.into())    
    .map_err(|e| e.into())
   
}



/// calculates the deadline at some epoch for a proving period
/// and returns the deadline-related calculations.
pub fn state_miner_proving_deadline<DB: BlockStore + Send + Sync + 'static,  KS: KeyStore + Send + Sync + 'static>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(AddressJson,TipsetKeys)>,
) -> Result<DeadlineInfo, JsonRpcError>
{
    let state_manager = StateManager::new(data.store);
    let (address_json,key) = params;
    let actor = address_json.into();
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    let miner_actor_state: State =
        state_manager.load_actor_state(&actor, &tipset.parent_state())?;
    Ok(compute_proving_period_deadline(
        miner_actor_state.proving_period_start,
        tipset.epoch(),
    ).into())
   
}

/// returns a single non-expired Faults that occur within lookback epochs of the given tipset
pub fn state_miner_faults<DB: BlockStore + Send + Sync + 'static,  KS: KeyStore + Send + Sync + 'static>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(AddressJson,TipsetKeys)>,
) -> Result<BitFieldJson, JsonRpcError>
{
    let state_manager = StateManager::new(data.store);
    let (address_json,key) = params;
    let actor = address_json.into();
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    state_manager::utils::get_miner_faults(&state_manager, &tipset, actor)
    .map(|s|s.into())
    .map_err(|e| e.into())
}

/// returns all non-expired Faults that occur within lookback epochs of the given tipset
pub fn state_all_miner_faults<DB: BlockStore + Send + Sync + 'static,  KS: KeyStore + Send + Sync + 'static>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(ChainEpoch,TipsetKeys)>,
) -> Result<Vec<Fault>, JsonRpcError>
{
    let state_manager = StateManager::new(data.store);
    let (look_back,key) = params;
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
    Ok(all_faults.into())
}
/// returns a bitfield indicating the recovering sectors of the given miner
pub fn state_miner_recoveries<DB: BlockStore + Send + Sync + 'static,  KS: KeyStore + Send + Sync + 'static>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(AddressJson,TipsetKeys)>,
) -> Result<BitFieldJson, JsonRpcError>
{
    let state_manager = StateManager::new(data.store);
    let (address_json,key) = params;
    let address = address_json.into();
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    state_manager::utils::get_miner_recoveries(&state_manager, &tipset, actor).map_err(|e| e.into())
}

/// returns the power of the indicated miner
pub fn state_miner_power<DB: BlockStore + Send + Sync + 'static,  KS: KeyStore + Send + Sync + 'static>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(AddressJson,TipsetKeys)>,
) -> Result<BitFieldJson, JsonRpcError>
{
    let state_manager = StateManager::new(data.store);
    let (address_json,key) = params;
    let actor = address_json.into();
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    state_manager::utils::get_power(&state_manager, &tipset, Some(actor))
    .map(|s|s.into())
    .map_err(|e| e.into())

}

/// returns the result of executing the indicated message, assuming it was executed in the indicated tipset.
pub fn state_reply<DB: BlockStore + Send + Sync + 'static,  KS: KeyStore + Send + Sync + 'static>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(CidJson,TipsetKeys)>,
) -> Result<InvocResult<UnsignedMessageJson>, JsonRpcError>
{
    let state_manager = StateManager::new(data.store);
    let (cidjson,key) = params;
    let cid = cidjson.into();
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    let (msg, ret) = state_manager.replay(&tipset, cid)?;

    Ok(InvocResult {
        msg,
        msg_rct: ret.clone().map(|s| s.msg_receipt().clone()),
        actor_error: ret
            .map(|act| act.act_error().map(|e| e.to_string()))
            .unwrap_or_default(),
    }.into())
}

/// returns the indicated actor's nonce and balance.
pub fn state_get_actor<DB: BlockStore + Send + Sync + 'static,  KS: KeyStore + Send + Sync + 'static>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(AddressJson,TipsetKeys)>,
) -> Result<Option<actor::ActorState>, JsonRpcError>
{
    let state_manager = StateManager::new(data.store);
    let (address_json,key) = params;
    let actor = address_json.into();
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    let state = state_for_ts(&state_manager, Some(tipset))?;
    state.get_actor(actor)
    .map(|s|s.into())
    .map_err(|e| e.into())
}

/// returns the public key address of the given ID address
pub fn state_account_key<DB: BlockStore + Send + Sync + 'static,  KS: KeyStore + Send + Sync + 'static>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(AddressJson,TipsetKeys)>,
) -> Result<Option<actor::ActorState>, JsonRpcError>
{
    let state_manager = StateManager::new(data.store);
    let (address_json,key) = params;
    let actor = address_json.into();
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    let state = state_for_ts(&state_manager, Some(tipset))?;
    let address =
        interpreter::resolve_to_key_addr(&state, state_manager.get_block_store_ref(), actor)?;
    Ok(address.into())
}
/// retrieves the ID address of the given address
pub fn state_lookup_id<DB: BlockStore + Send + Sync + 'static,  KS: KeyStore + Send + Sync + 'static>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(AddressJson,TipsetKeys)>,
) -> Result<Option<actor::ActorState>, JsonRpcError>
{
    let state_manager = StateManager::new(data.store);
    let (address_json,key) = params;
    let address = address_json.into();
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    let state = state_for_ts(&state_manager, Some(tipset))?;
    state.lookup_id(address)
    .map(|s|s.into())
    .map_err(|e| e.into())
}

/// looks up the Escrow and Locked balances of the given address in the Storage Market
pub fn state_market_balance<DB: BlockStore + Send + Sync + 'static,  KS: KeyStore + Send + Sync + 'static>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(AddressJson,TipsetKeys)>,
) -> Result<Option<actor::ActorState>, JsonRpcError>
{
    let state_manager = StateManager::new(data.store);
    let (address_json,key_json) = params;
    let address = address_json.into();
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    state_manager
        .market_balance(address, &tipset)
        .map(|s|s.into())
        .map_err(|e| e.into())
}

/// returns the message receipt for the given message
pub fn state_get_receipt<DB: BlockStore + Send + Sync + 'static,  KS: KeyStore + Send + Sync + 'static>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(CidJson,TipsetKeys)>,
) -> Result<Option<actor::ActorState>, JsonRpcError>
{
    let (cidjson,key) = params;
    let state_manager = StateManager::new(data.store);
    let cid  = cidjson.into();
    let tipset = ChainStore::new(state_manager.get_block_store()).tipset_from_keys(key)?;
    state_manager
        .get_receipt(&tipset, msg)
        .map(|s|s.into())
        .map_err(|e| e.into())
}
/// looks back in the chain for a message. If not found, it blocks until the
/// message arrives on chain, and gets to the indicated confidence depth.
pub fn state_wait_msg<DB: BlockStore + Send + Sync + 'static,  KS: KeyStore + Send + Sync + 'static>(
    data: Data<RpcState<DB, KS>>,
    Params(params): Params<(CidJson,u64)>,
) -> Result<MessageLookup, JsonRpcError>
{
    let (cidjson,confidence) = params;
    let state_manager = StateManager::new(data.store);
    let cid = cidjson.into();
    let maybe_tuple = task::block_on(state_manager.wait_for_message(cid, confidence))?;
    let (tipset, receipt) = maybe_tuple.ok_or_else(|| "wait for msg returned empty tuple")?;
    Ok(MessageLookup { receipt, tipset }.into())
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
