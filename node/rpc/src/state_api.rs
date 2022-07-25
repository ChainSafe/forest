// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use chain::Scale;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use std::collections::HashMap;
use std::sync::Arc;

use actor::{
    market,
    miner::{self, MinerPower, SectorOnChainInfo},
    power::{self, Claim},
    reward,
};
use beacon::{Beacon, BeaconEntry};
use cid::Cid;
use fil_types::{
    verifier::{FullVerifier, ProofVerifier},
    PoStProof,
};
use forest_blocks::{
    election_proof::json::ElectionProofJson, ticket::json::TicketJson,
    tipset_keys_json::TipsetKeysJson,
};
use forest_blocks::{
    gossip_block::json::GossipBlockJson as BlockMsgJson, BlockHeader, GossipBlock as BlockMsg,
    Tipset,
};
use forest_ipld::{json::IpldJson, Ipld};
use forest_json::address::json::AddressJson;
use forest_json::cid::CidJson;
use forest_message::signed_message::SignedMessage;
use fvm::state_tree::StateTree;
use fvm_shared::{address::Address, bigint::BigInt};
use ipld_blockstore::{BlockStore, BlockStoreExt};
use networks::Height;
use rpc_api::{
    data_types::{
        ActorStateJson, Deadline, MarketDeal, MessageLookup, MiningBaseInfoJson, Partition,
        RPCState,
    },
    state_api::*,
};
use state_manager::{InvocResult, StateManager};

// TODO handle using configurable verification implementation in RPC (all defaulting to Full).

/// returns info about the given miner's sectors. If the filter bitfield is nil, all sectors are included.
/// If the filterOut boolean is set to true, any sectors in the filter are excluded.
/// If false, only those sectors in the filter are included.
pub(crate) async fn state_miner_sectors<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateMinerSectorsParams>,
) -> Result<StateMinerSectorsResult, JsonRpcError> {
    let (address, filter, key) = params;
    let address = address.into();
    let state_manager = &data.state_manager;
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    let filter = Some(&filter.0);
    state_manager
        .get_miner_sector_set::<FullVerifier>(&tipset, &address, filter)
        .map_err(|e| e.into())
}

/// runs the given message and returns its result without any persisted changes.
pub(crate) async fn state_call<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateCallParams>,
) -> Result<StateCallResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (message_json, key) = params;
    let mut message = message_json.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    Ok(state_manager.call(&mut message, Some(tipset)).await?)
}

/// returns all the proving deadlines for the given miner
pub(crate) async fn state_miner_deadlines<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateMinerDeadlinesParams>,
) -> anyhow::Result<StateMinerDeadlinesResult, JsonRpcError> {
    let (actor, key) = params;
    let actor = actor.into();
    let mas = data
        .state_manager
        .chain_store()
        .miner_load_actor_tsk(&actor, &key.into())
        .await
        .map_err(|e| format!("Could not load miner {:?}", e))?;

    let num_deadlines = data
        .state_manager
        .chain_config()
        .policy
        .wpost_period_deadlines;
    let mut out = Vec::with_capacity(num_deadlines as usize);
    mas.for_each_deadline(data.state_manager.blockstore(), |_, dl| {
        out.push(Deadline {
            post_submissions: dl.partitions_posted().clone().into(),
            disputable_proof_count: dl.disputable_proof_count(data.state_manager.blockstore())?,
        });
        Ok(())
    })?;

    Ok(out)
}

/// returns the PreCommit info for the specified miner's sector
pub(crate) async fn state_sector_precommit_info<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateSectorPrecommitInfoParams>,
) -> Result<StateSectorPrecommitInfoResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (address, sector_number, key) = params;
    let address = address.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.0)
        .await?;
    state_manager
        .precommit_info::<FullVerifier>(&address, &sector_number, &tipset)
        .map_err(|e| e.into())
}

/// StateMinerInfo returns info about the indicated miner
pub async fn state_miner_info<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateMinerInfoParams>,
) -> Result<StateMinerInfoResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let store = state_manager.blockstore();
    let (AddressJson(addr), TipsetKeysJson(key)) = params;

    let ts = data.chain_store.tipset_from_keys(&key).await?;
    let actor = data
        .state_manager
        .get_actor(&addr, *ts.parent_state())
        .map_err(|e| format!("Could not load miner {}: {:?}", addr, e))?
        .ok_or_else(|| format!("miner {} does not exist", addr))?;

    let miner_state = miner::State::load(store, &actor)?;

    let miner_info = miner_state
        .info(store)
        .map_err(|e| format!("Could not get info {:?}", e))?;

    Ok(miner_info)
}

/// returns the on-chain info for the specified miner's sector
pub async fn state_sector_info<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateSectorGetInfoParams>,
) -> Result<StateSectorGetInfoResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (address, sector_number, key) = params;
    let address = address.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    state_manager
        .miner_sector_info::<FullVerifier>(&address, sector_number, &tipset)
        .map_err(|e| e.into())
        .map(|e| e.map(SectorOnChainInfo::from))
}

/// calculates the deadline at some epoch for a proving period
/// and returns the deadline-related calculations.
pub(crate) async fn state_miner_proving_deadline<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateMinerProvingDeadlineParams>,
) -> Result<StateMinerProvingDeadlineResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (AddressJson(addr), key) = params;
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;

    let actor = state_manager
        .get_actor(&addr, *tipset.parent_state())?
        .ok_or_else(|| format!("Address {} not found", addr))?;

    let mas = miner::State::load(state_manager.blockstore(), &actor)?;

    Ok(mas.deadline_info(tipset.epoch()).next_not_elapsed())
}

/// returns a single non-expired Faults that occur within lookback epochs of the given tipset
pub(crate) async fn state_miner_faults<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateMinerFaultsParams>,
) -> Result<StateMinerFaultsResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (actor, key) = params;
    let actor = actor.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    state_manager
        .get_miner_faults::<FullVerifier>(&tipset, &actor)
        .map(|s| s.into())
        .map_err(|e| e.into())
}

/// returns all non-expired Faults that occur within lookback epochs of the given tipset
pub(crate) async fn state_all_miner_faults<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    _data: Data<RPCState<DB, B>>,
    Params(_params): Params<StateAllMinerFaultsParams>,
) -> Result<StateAllMinerFaultsResult, JsonRpcError> {
    // FIXME
    Err(JsonRpcError::internal("fixme"))

    // let state_manager = &data.state_manager;
    // let (look_back, end_tsk) = params;
    // let tipset = data.state_manager.chain_store().tipset_from_keys( &end_tsk).await?;
    // let cut_off = tipset.epoch() - look_back;
    // let miners = state_manager.list_miner_actors(&tipset)?;
    // let mut all_faults = Vec::new();
    // for m in miners {
    //     let miner_actor_state: State = state_manager
    //         .load_actor_state(&m, &tipset.parent_state())
    //         .map_err(|e| e.to_string())?;
    //     let block_store = state_manager.blockstore();

    //     miner_actor_state.for_each_fault_epoch(block_store, |fault_start: i64, _| {
    //         if fault_start >= cut_off {
    //             all_faults.push(Fault {
    //                 miner: *m,
    //                 fault: fault_start,
    //             })
    //         }
    //         Ok(())
    //     })?;
    // }
    // Ok(all_faults)
}

/// returns a bitfield indicating the recovering sectors of the given miner
pub(crate) async fn state_miner_recoveries<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateMinerRecoveriesParams>,
) -> Result<StateMinerRecoveriesResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (actor, key) = params;
    let actor = actor.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    state_manager
        .get_miner_recoveries::<FullVerifier>(&tipset, &actor)
        .map(|s| s.into())
        .map_err(|e| e.into())
}

/// returns a bitfield indicating the recovering sectors of the given miner
pub(crate) async fn state_miner_partitions<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateMinerPartitionsParams>,
) -> anyhow::Result<StateMinerPartitionsResult, JsonRpcError> {
    let (actor, dl_idx, key) = params;
    let actor = actor.into();
    let db = data.state_manager.chain_store().db.as_ref();
    let mas = data
        .state_manager
        .chain_store()
        .miner_load_actor_tsk(&actor, &key.into())
        .await
        .map_err(|e| format!("Could not load miner {:?}", e))?;
    let dl = mas.load_deadline(db, dl_idx)?;
    let mut out = Vec::new();
    dl.for_each(db, |_, part| {
        out.push(Partition {
            all_sectors: part.all_sectors().clone().into(),
            faulty_sectors: part.faulty_sectors().clone().into(),
            recovering_sectors: part.recovering_sectors().clone().into(),
            live_sectors: part.live_sectors().into(),
            active_sectors: part.active_sectors().into(),
        });
        Ok(())
    })?;

    Ok(out)
}

/// returns the result of executing the indicated message, assuming it was executed in the indicated tipset.
pub(crate) async fn state_replay<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateReplayParams>,
) -> Result<StateReplayResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (cidjson, key) = params;
    let cid = cidjson.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    let (msg, ret) = state_manager.replay(&tipset, cid).await?;

    Ok(InvocResult {
        msg,
        msg_rct: Some(ret.msg_receipt),
        error: ret.failure_info.map(|e| e.to_string()),
    })
}

/// gets network name from state manager
pub(crate) async fn state_network_name<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
) -> Result<StateNetworkNameResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let heaviest_tipset = state_manager
        .chain_store()
        .heaviest_tipset()
        .await
        .ok_or("Heaviest Tipset not found in state_network_name")?;

    state_manager
        .get_network_name(heaviest_tipset.parent_state())
        .map_err(|e| e.into())
}

pub(crate) async fn state_get_network_version<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateNetworkVersionParams>,
) -> Result<StateNetworkVersionResult, JsonRpcError> {
    let (TipsetKeysJson(tsk),) = params;
    let ts = data.chain_store.tipset_from_keys(&tsk).await?;
    Ok(data.state_manager.get_network_version(ts.epoch()))
}

/// returns the indicated actor's nonce and balance.
pub(crate) async fn state_get_actor<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateGetActorParams>,
) -> Result<StateGetActorResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (actor, key) = params;
    let actor = actor.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    let state = state_for_ts(state_manager, tipset).await?;
    Ok(state.get_actor(&actor)?.map(ActorStateJson::from))
}

/// returns addresses of all actors on the network by tipset
pub(crate) async fn state_list_actors<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateListActorsParams>,
) -> Result<StateListActorsResult, JsonRpcError> {
    let (tipset,) = params;

    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&tipset.into())
        .await?;

    let addresses = data.state_manager.list_miner_actors(&tipset)?;

    let addresses_json = addresses
        .iter()
        .map(|addr| AddressJson(addr.to_owned()))
        .collect();

    Ok(addresses_json)
}

/// returns the public key address of the given ID address
pub(crate) async fn state_account_key<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateAccountKeyParams>,
) -> Result<StateAccountKeyResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (actor, key) = params;
    let actor = actor.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    let state = state_for_ts(state_manager, tipset).await?;
    let address = interpreter::resolve_to_key_addr(&state, state_manager.blockstore(), &actor)?;
    Ok(Some(address.into()))
}
/// retrieves the ID address of the given address
pub(crate) async fn state_lookup_id<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateLookupIdParams>,
) -> Result<StateLookupIdResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (address, key) = params;
    let address = address.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    let state = state_for_ts::<DB>(state_manager, tipset).await?;
    let lookup_result = state.lookup_id(&address)?;

    match lookup_result {
        Some(actor_id) => Ok(Some(AddressJson(Address::new_id(actor_id)))),
        None => Ok(None),
    }
}

/// looks up the Escrow and Locked balances of the given address in the Storage Market
pub(crate) async fn state_market_balance<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateMarketBalanceParams>,
) -> Result<StateMarketBalanceResult, JsonRpcError> {
    let (address, key) = params;
    let address = address.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    data.state_manager
        .market_balance(&address, &tipset)
        .map_err(|e| e.into())
}

pub(crate) async fn state_market_deals<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateMarketDealsParams>,
) -> Result<StateMarketDealsResult, JsonRpcError> {
    let (TipsetKeysJson(tsk),) = params;
    let ts = data.chain_store.tipset_from_keys(&tsk).await?;
    let actor = data
        .state_manager
        .get_actor(&market::ADDRESS, *ts.parent_state())?
        .ok_or("Market actor address could not be resolved")?;
    let market_state = market::State::load(data.state_manager.blockstore(), &actor)?;

    let da = market_state.proposals(data.state_manager.blockstore())?;
    let sa = market_state.states(data.state_manager.blockstore())?;

    let mut out = HashMap::new();
    da.for_each(|deal_id, d| {
        let s = sa.get(deal_id)?.unwrap_or(market::DealState {
            sector_start_epoch: -1,
            last_updated_epoch: -1,
            slash_epoch: -1,
        });
        out.insert(
            deal_id.to_string(),
            MarketDeal {
                proposal: d,
                state: s,
            },
        );
        Ok(())
    })?;
    Ok(out)
}

/// returns the message receipt for the given message
pub(crate) async fn state_get_receipt<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateGetReceiptParams>,
) -> Result<StateGetReceiptResult, JsonRpcError> {
    let (cidjson, key) = params;
    let state_manager = &data.state_manager;
    let cid = cidjson.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    state_manager
        .get_receipt(&tipset, cid)
        .await
        .map(|s| s.into())
        .map_err(|e| e.into())
}
/// looks back in the chain for a message. If not found, it blocks until the
/// message arrives on chain, and gets to the indicated confidence depth.
pub(crate) async fn state_wait_msg<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateWaitMsgParams>,
) -> Result<StateWaitMsgResult, JsonRpcError> {
    let (cidjson, confidence) = params;
    let state_manager = &data.state_manager;
    let cid: Cid = cidjson.into();
    let (tipset, receipt) = state_manager.wait_for_message(cid, confidence).await?;
    let tipset = tipset.ok_or("wait for msg returned empty tuple")?;
    let receipt = receipt.ok_or("wait for msg returned empty receipt")?;
    let ipld: Ipld = if receipt.return_data.bytes().is_empty() {
        Ipld::Null
    } else {
        receipt.return_data.deserialize()?
    };
    Ok(MessageLookup {
        receipt: receipt.into(),
        tipset: tipset.key().clone().into(),
        height: tipset.epoch(),
        message: CidJson(cid),
        return_dec: IpldJson(ipld),
    })
}

pub(crate) async fn miner_create_block<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
    S: Scale,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<MinerCreateBlockParams>,
) -> Result<MinerCreateBlockResult, JsonRpcError> {
    let params = params.0;
    let AddressJson(miner) = params.miner;
    let TipsetKeysJson(parents) = params.parents;
    let TicketJson(ticket) = params.ticket;
    let ElectionProofJson(eproof) = params.eproof;
    let beacon_values: Vec<BeaconEntry> = params.beacon_values.into_iter().map(|b| b.0).collect();
    let messages: Vec<SignedMessage> = params.messages.into_iter().map(|m| m.0).collect();
    let epoch = params.epoch;
    let timestamp = params.timestamp;
    let winning_post_proof: Vec<PoStProof> = params
        .winning_post_proof
        .into_iter()
        .map(|wpp| wpp.0)
        .collect();

    let pts = data.chain_store.tipset_from_keys(&parents).await?;
    let (st, recpts) = data.state_manager.tipset_state(&pts).await?;
    let (_, lbst) = data
        .state_manager
        .get_lookback_tipset_for_round(pts.clone(), epoch)
        .await?;
    let worker = data.state_manager.get_miner_work_addr(lbst, &miner)?;

    let persisted =
        chain::persist_block_messages(data.chain_store.blockstore(), messages.iter().collect())?;

    let pweight = S::weight(data.chain_store.blockstore(), pts.as_ref())?;
    let smoke_height = data.state_manager.chain_config().epoch(Height::Smoke);
    let base_fee =
        chain::compute_base_fee(data.chain_store.blockstore(), pts.as_ref(), smoke_height)?;

    let mut next = BlockHeader::builder()
        .messages(persisted.msg_cid)
        .bls_aggregate(Some(persisted.bls_agg))
        .miner_address(miner)
        .weight(pweight)
        .parent_base_fee(base_fee)
        .parents(parents)
        .ticket(Some(ticket))
        .election_proof(Some(eproof))
        .beacon_entries(beacon_values)
        .epoch(epoch)
        .timestamp(timestamp)
        .winning_post_proof(winning_post_proof)
        .state_root(st)
        .message_receipts(recpts)
        .signature(None)
        .build()?;

    let key = key_management::find_key(&worker, &*data.keystore.as_ref().read().await)?;
    let sig = key_management::sign(
        *key.key_info.key_type(),
        key.key_info.private_key(),
        &next.to_signing_bytes(),
    )?;
    next.signature = Some(sig);

    Ok(BlockMsgJson(BlockMsg {
        header: next,
        bls_messages: persisted.bls_cids,
        secpk_messages: persisted.secp_cids,
    }))
}

pub(crate) async fn state_miner_sector_allocated<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateMinerSectorAllocatedParams>,
) -> Result<StateMinerSectorAllocatedResult, JsonRpcError> {
    let (AddressJson(maddr), sector_num, TipsetKeysJson(tsk)) = params;
    let ts = data.chain_store.tipset_from_keys(&tsk).await?;

    let actor = data
        .state_manager
        .get_actor(&maddr, *ts.parent_state())?
        .ok_or(format!("Miner actor {} could not be resolved", maddr))?;
    let allocated_sectors = match miner::State::load(data.state_manager.blockstore(), &actor)? {
        // miner::State::V0(m) => data
        //     .chain_store
        //     .db
        //     .get::<bitfield::BitField>(&m.allocated_sectors)?
        //     .ok_or("allocated sectors bitfield not found")?
        //     .get(sector_num as usize),
        // miner::State::V2(m) => data
        //     .chain_store
        //     .db
        //     .get::<bitfield::BitField>(&m.allocated_sectors)?
        //     .ok_or("allocated sectors bitfield not found")?
        //     .get(sector_num as usize),
        // miner::State::V3(m) => data
        //     .chain_store
        //     .db
        //     .get::<bitfield::BitField>(&m.allocated_sectors)?
        //     .ok_or("allocated sectors bitfield not found")?
        //     .get(sector_num as usize),
        // miner::State::V4(m) => data
        //     .chain_store
        //     .db
        //     .get::<bitfield::BitField>(&m.allocated_sectors)?
        //     .ok_or("allocated sectors bitfield not found")?
        //     .get(sector_num as usize),
        // miner::State::V5(m) => data
        //     .chain_store
        //     .db
        //     .get::<bitfield::BitField>(&m.allocated_sectors)?
        //     .ok_or("allocated sectors bitfield not found")?
        //     .get(sector_num as usize),
        // miner::State::V6(m) => data
        //     .chain_store
        //     .db
        //     .get::<bitfield::BitField>(&m.allocated_sectors)?
        //     .ok_or("allocated sectors bitfield not found")?
        //     .get(sector_num as usize),
        miner::State::V8(m) => data
            .chain_store
            .db
            .get_obj::<fvm_ipld_bitfield::BitField>(&m.allocated_sectors)?
            .ok_or("allocated sectors bitfield not found")?
            .get(sector_num),
    };

    Ok(allocated_sectors)
}

pub(crate) async fn state_miner_power<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateMinerPowerParams>,
) -> Result<StateMinerPowerResult, JsonRpcError> {
    let (address_opt, TipsetKeysJson(tipset_keys)) = params;

    let ts = data.chain_store.tipset_from_keys(&tipset_keys).await?;

    let address = match address_opt {
        Some(addr) => {
            let address = addr.0;
            Some(address)
        }
        None => None,
    };

    let mp = data
        .state_manager
        .get_power(ts.parent_state(), address.as_ref())?;

    if let Some((miner_power, total_power)) = mp {
        Ok(MinerPower {
            miner_power,
            total_power,
            has_min_power: true,
        })
    } else {
        Ok(MinerPower {
            miner_power: Claim::default(),
            total_power: Claim::default(),
            has_min_power: false,
        })
    }
}

pub(crate) async fn state_miner_pre_commit_deposit_for_power<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateMinerPreCommitDepositForPowerParams>,
) -> Result<StateMinerPreCommitDepositForPowerResult, JsonRpcError> {
    let (AddressJson(maddr), pci, TipsetKeysJson(tsk)) = params;
    let ts = data.chain_store.tipset_from_keys(&tsk).await?;
    let (state, _) = data.state_manager.tipset_state(&ts).await?;
    let state = StateTree::new_from_root(data.chain_store.db.as_ref(), &state)?;
    let ssize = pci.seal_proof.sector_size()?;

    let actor = state
        .get_actor(&market::ADDRESS)?
        .ok_or("couldnt load market actor")?;
    let (w, vw) = market::State::load(data.state_manager.blockstore(), &actor)?
        .verify_deals_for_activation(
            data.state_manager.blockstore(),
            &pci.deal_ids,
            &maddr,
            pci.expiration,
            ts.epoch(),
        )?;
    let duration = pci.expiration - ts.epoch();
    let sector_weight = fil_actor_miner_v8::qa_power_for_weight(ssize, duration, &w, &vw);

    let actor = state
        .get_actor(&power::ADDRESS)?
        .ok_or("couldnt load power actor")?;
    let power_smoothed =
        power::State::load(data.state_manager.blockstore(), &actor)?.total_power_smoothed();

    let reward_actor = state
        .get_actor(&reward::ADDRESS)?
        .ok_or("couldnt load reward actor")?;
    let deposit = reward::State::load(data.state_manager.blockstore(), &reward_actor)?
        .pre_commit_deposit_for_power(power_smoothed, &sector_weight);

    let ret: BigInt = (deposit * 110) / 100;
    Ok(ret.to_string())
}

pub(crate) async fn state_miner_initial_pledge_collateral<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateMinerInitialPledgeCollateralParams>,
) -> Result<StateMinerInitialPledgeCollateralResult, JsonRpcError> {
    let (AddressJson(maddr), pci, TipsetKeysJson(tsk)) = params;
    let ts = data.chain_store.tipset_from_keys(&tsk).await?;
    let (root_cid, _) = data.state_manager.tipset_state(&ts).await?;
    let state = StateTree::new_from_root(data.chain_store.db.as_ref(), &root_cid)?;
    let ssize = pci.seal_proof.sector_size()?;

    let actor = state
        .get_actor(&market::ADDRESS)?
        .ok_or("couldnt load market actor")?;
    let (w, vw) = market::State::load(data.state_manager.blockstore(), &actor)?
        .verify_deals_for_activation(
            data.state_manager.blockstore(),
            &pci.deal_ids,
            &maddr,
            pci.expiration,
            ts.epoch(),
        )?;
    let duration = pci.expiration - ts.epoch();
    let sector_weight = fil_actor_miner_v8::qa_power_for_weight(ssize, duration, &w, &vw);

    let actor = state
        .get_actor(&power::ADDRESS)?
        .ok_or("couldnt load power actor")?;
    let power_state = power::State::load(data.state_manager.blockstore(), &actor)?;
    let power_smoothed = power_state.total_power_smoothed();
    let total_locked = power_state.total_locked();

    let circ_supply = data
        .state_manager
        .get_circulating_supply(ts.epoch(), &state)?;

    let reward_actor = state
        .get_actor(&reward::ADDRESS)?
        .ok_or("couldnt load reward actor")?;

    let initial_pledge = reward::State::load(data.state_manager.blockstore(), &reward_actor)?
        .initial_pledge_for_power(&sector_weight, &total_locked, power_smoothed, &circ_supply);

    let ret: BigInt = (initial_pledge * 110) / 100;
    Ok(ret.to_string())
}

/// returns the indicated actor's nonce and balance.
pub(crate) async fn miner_get_base_info<
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
    V: ProofVerifier + Send + Sync + 'static,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<MinerGetBaseInfoParams>,
) -> Result<MinerGetBaseInfoResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (actor, round, key) = params;
    let info = state_manager
        .miner_get_base_info::<V, B>(&data.beacon, &key.into(), round, actor.into())
        .await?
        .map(MiningBaseInfoJson::from);

    Ok(info)
}

/// returns a state tree given a tipset
async fn state_for_ts<DB>(
    state_manager: &Arc<StateManager<DB>>,
    ts: Arc<Tipset>,
) -> Result<StateTree<&DB>, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
{
    let block_store = state_manager.blockstore();
    let (st, _) = state_manager.tipset_state(&ts).await?;
    let state_tree = StateTree::new_from_root(block_store, &st)?;
    Ok(state_tree)
}
