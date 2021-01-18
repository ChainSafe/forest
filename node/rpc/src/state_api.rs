// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::RpcState;
use actor::{
    market::{self, DealProposal, DealState},
    miner::{self, MinerInfo, SectorOnChainInfo, SectorPreCommitInfo},
    power::{self},
    reward::{self},
};
use address::{json::AddressJson, Address};
use beacon::{json::BeaconEntryJson, Beacon, BeaconEntry};
use bitfield::json::BitFieldJson;
use blocks::{
    election_proof::json::ElectionProofJson, ticket::json::TicketJson,
    tipset_keys_json::TipsetKeysJson,
};
use blocks::{
    gossip_block::json::GossipBlockJson as BlockMsgJson, BlockHeader, GossipBlock as BlockMsg,
    Tipset, TxMeta,
};
use blockstore::BlockStore;
use bls_signatures::Serialize as SerializeBls;
use cid::{json::CidJson, Cid, Code::Blake2b256};
use clock::ChainEpoch;
use crypto::SignatureType;
use fil_types::json::SectorInfoJson;
use fil_types::sector::post::json::PoStProofJson;
use fil_types::{
    deadlines::DeadlineInfo,
    verifier::{FullVerifier, ProofVerifier},
    NetworkVersion, PoStProof, SectorNumber, SectorSize,
};
use ipld::{json::IpldJson, Ipld};
use ipld_amt::Amt;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use message::{
    message_receipt::json::MessageReceiptJson,
    signed_message::{json::SignedMessageJson, SignedMessage},
    unsigned_message::{json::UnsignedMessageJson, UnsignedMessage},
};
use networks::get_network_version_default;
use num_bigint::{bigint_ser, BigInt};
use serde::{Deserialize, Serialize};
use state_manager::{InvocResult, MarketBalance, MiningBaseInfo, StateManager};
use state_tree::StateTree;
use std::collections::HashMap;
use std::sync::Arc;
use vm::ActorState;
use wallet::KeyStore;

// TODO handle using configurable verification implementation in RPC (all defaulting to Full).

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MessageLookup {
    pub receipt: MessageReceiptJson,
    #[serde(rename = "TipSet")]
    pub tipset: TipsetKeysJson,
    pub height: i64,
    pub message: CidJson,
    pub return_dec: IpldJson,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Partition {
    all_sectors: BitFieldJson,
    faulty_sectors: BitFieldJson,
    recovering_sectors: BitFieldJson,
    live_sectors: BitFieldJson,
    active_sectors: BitFieldJson,
}

/// returns info about the given miner's sectors. If the filter bitfield is nil, all sectors are included.
/// If the filterOut boolean is set to true, any sectors in the filter are excluded.
/// If false, only those sectors in the filter are included.
pub(crate) async fn state_miner_sectors<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(AddressJson, BitFieldJson, TipsetKeysJson)>,
) -> Result<Vec<miner::SectorOnChainInfo>, JsonRpcError> {
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
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(UnsignedMessageJson, TipsetKeysJson)>,
) -> Result<InvocResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (unsigned_msg_json, key) = params;
    let mut message: UnsignedMessage = unsigned_msg_json.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    Ok(state_manager
        .call::<FullVerifier>(&mut message, Some(tipset))
        .await?)
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct Deadline {
    post_submissions: BitFieldJson,
}

/// returns all the proving deadlines for the given miner
pub(crate) async fn state_miner_deadlines<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(AddressJson, TipsetKeysJson)>,
) -> Result<Vec<Deadline>, JsonRpcError> {
    let (actor, key) = params;
    let actor = actor.into();
    let mas = data
        .state_manager
        .chain_store()
        .miner_load_actor_tsk(&actor, &key.into())
        .await
        .map_err(|e| format!("Could not load miner {:?}", e))?;

    let mut out = Vec::with_capacity(mas.num_deadlines() as usize);
    mas.for_each_deadline(data.state_manager.blockstore(), |_, dl| {
        let ps = dl.into_post_submissions();
        out.push(Deadline {
            post_submissions: BitFieldJson(ps),
        });
        Ok(())
    })?;

    Ok(out)
}

/// returns the PreCommit info for the specified miner's sector
pub(crate) async fn state_sector_precommit_info<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(AddressJson, SectorNumber, TipsetKeysJson)>,
) -> Result<miner::SectorPreCommitOnChainInfo, JsonRpcError> {
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
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(AddressJson, TipsetKeysJson)>,
) -> Result<MinerInfo, JsonRpcError> {
    let state_manager = &data.state_manager;
    let store = state_manager.blockstore();
    let (AddressJson(addr), TipsetKeysJson(key)) = params;

    let ts = data.chain_store.tipset_from_keys(&key).await?;
    let actor = data
        .state_manager
        .get_actor(&addr, ts.parent_state())
        .map_err(|e| format!("Could not load miner {}: {:?}", addr, e))?
        .ok_or_else(|| format!("miner {} does not exist", addr))?;

    let miner_state = miner::State::load(store, &actor)?;

    let mut miner_info = miner_state
        .info(store)
        .map_err(|e| format!("Could not get info {:?}", e))?;

    // TODO revisit better way of handling (Lotus does here as well)
    if get_network_version_default(ts.epoch()) >= NetworkVersion::V7 {
        miner_info.seal_proof_type.update_to_v1();
    }
    Ok(miner_info)
}

/// returns the on-chain info for the specified miner's sector
pub async fn state_sector_info<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(AddressJson, SectorNumber, TipsetKeysJson)>,
) -> Result<Option<SectorOnChainInfo>, JsonRpcError> {
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
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(AddressJson, TipsetKeysJson)>,
) -> Result<DeadlineInfo, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (AddressJson(addr), key) = params;
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;

    let actor = state_manager
        .get_actor(&addr, &tipset.parent_state())?
        .ok_or_else(|| format!("Address {} not found", addr))?;

    let mas = miner::State::load(state_manager.blockstore(), &actor)?;

    Ok(mas.deadline_info(tipset.epoch()).next_not_elapsed())
}

/// returns a single non-expired Faults that occur within lookback epochs of the given tipset
pub(crate) async fn state_miner_faults<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(AddressJson, TipsetKeysJson)>,
) -> Result<BitFieldJson, JsonRpcError> {
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

#[derive(Serialize)]
pub struct Fault {
    pub miner: Address,
    pub epoch: ChainEpoch,
}

/// returns all non-expired Faults that occur within lookback epochs of the given tipset
pub(crate) async fn state_all_miner_faults<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    _data: Data<RpcState<DB, KS, B>>,
    Params(_params): Params<(ChainEpoch, TipsetKeysJson)>,
) -> Result<Vec<Fault>, JsonRpcError> {
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
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(AddressJson, TipsetKeysJson)>,
) -> Result<BitFieldJson, JsonRpcError> {
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
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(AddressJson, u64, TipsetKeysJson)>,
) -> Result<Vec<Partition>, JsonRpcError> {
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
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(CidJson, TipsetKeysJson)>,
) -> Result<InvocResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (cidjson, key) = params;
    let cid = cidjson.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    let (msg, ret) = state_manager.replay::<FullVerifier>(&tipset, cid).await?;

    Ok(InvocResult {
        msg,
        msg_rct: Some(ret.msg_receipt),
        error: ret.act_error.map(|e| e.to_string()),
    })
}

pub(crate) async fn state_get_network_version<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(TipsetKeysJson,)>,
) -> Result<NetworkVersion, JsonRpcError> {
    let (TipsetKeysJson(tsk),) = params;
    let ts = data.chain_store.tipset_from_keys(&tsk).await?;
    Ok(data.state_manager.get_network_version(ts.epoch()))
}

/// returns the indicated actor's nonce and balance.
pub(crate) async fn state_get_actor<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
    V: ProofVerifier + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(AddressJson, TipsetKeysJson)>,
) -> Result<Option<ActorStateJson>, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (actor, key) = params;
    let actor = actor.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    let state = state_for_ts::<DB, V>(&state_manager, tipset).await?;
    Ok(state.get_actor(&actor)?.map(ActorStateJson::from))
}

/// returns the indicated actor's nonce and balance.
pub(crate) async fn state_miner_get_base_info<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
    V: ProofVerifier + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(AddressJson, ChainEpoch, TipsetKeysJson)>,
) -> Result<Option<MiningBaseInfoJson>, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (actor, round, key) = params;
    let info = state_manager
        .miner_get_base_info::<V, B>(&data.beacon, &key.into(), round, actor.into())
        .await?
        .map(MiningBaseInfoJson::from);

    Ok(info)
}
/// returns the public key address of the given ID address
pub(crate) async fn state_account_key<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
    V: ProofVerifier + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(AddressJson, TipsetKeysJson)>,
) -> Result<Option<AddressJson>, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (actor, key) = params;
    let actor = actor.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    let state = state_for_ts::<DB, V>(&state_manager, tipset).await?;
    let address = interpreter::resolve_to_key_addr(&state, state_manager.blockstore(), &actor)?;
    Ok(Some(address.into()))
}
/// retrieves the ID address of the given address
pub(crate) async fn state_lookup_id<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
    V: ProofVerifier + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(AddressJson, TipsetKeysJson)>,
) -> Result<Option<Address>, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (address, key) = params;
    let address = address.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    let state = state_for_ts::<DB, V>(&state_manager, tipset).await?;
    state.lookup_id(&address).map_err(|e| e.into())
}

/// gets network name from state manager
pub(crate) async fn state_network_name<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
) -> Result<String, JsonRpcError> {
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

/// looks up the Escrow and Locked balances of the given address in the Storage Market
pub(crate) async fn state_market_balance<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(AddressJson, TipsetKeysJson)>,
) -> Result<MarketBalance, JsonRpcError> {
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

/// returns the message receipt for the given message
pub(crate) async fn state_get_receipt<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(CidJson, TipsetKeysJson)>,
) -> Result<MessageReceiptJson, JsonRpcError> {
    let (cidjson, key) = params;
    let state_manager = &data.state_manager;
    let cid = cidjson.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    state_manager
        .get_receipt(&tipset, &cid)
        .await
        .map(|s| s.into())
        .map_err(|e| e.into())
}
/// looks back in the chain for a message. If not found, it blocks until the
/// message arrives on chain, and gets to the indicated confidence depth.
pub(crate) async fn state_wait_msg<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(CidJson, i64)>,
) -> Result<MessageLookup, JsonRpcError> {
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
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct BlockTemplate {
    miner: AddressJson,
    parents: TipsetKeysJson,
    ticket: TicketJson,
    eproof: ElectionProofJson,
    beacon_values: Vec<BeaconEntryJson>,
    messages: Vec<SignedMessageJson>,
    epoch: i64,
    timestamp: u64,
    #[serde(rename = "WinningPoStProof")]
    winning_post_proof: Vec<PoStProofJson>,
}

pub(crate) async fn miner_create_block<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
    V: ProofVerifier + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(BlockTemplate,)>,
) -> Result<BlockMsgJson, JsonRpcError> {
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
    let (st, recpts) = data.state_manager.tipset_state::<V>(&pts).await?;
    let (_, lbst) = data
        .state_manager
        .get_lookback_tipset_for_round::<V>(pts.clone(), epoch)
        .await?;
    let worker = data.state_manager.get_miner_worker_raw(&lbst, &miner)?;

    let mut bls_msgs = Vec::new();
    let mut secp_msgs = Vec::new();
    let mut bls_cids = Vec::new();
    let mut secp_cids = Vec::new();

    let mut bls_sigs = Vec::new();
    for msg in messages {
        if msg.signature().signature_type() == SignatureType::BLS {
            let c = data
                .chain_store
                .blockstore()
                .put(&msg.message, Blake2b256)?;
            bls_sigs.push(msg.signature);
            bls_msgs.push(msg.message);
            bls_cids.push(c);
        } else {
            let c = data.chain_store.blockstore().put(&msg, Blake2b256)?;
            secp_cids.push(c);
            secp_msgs.push(msg);
        }
    }

    let bls_msg_root = Amt::new_from_slice(data.chain_store.blockstore(), &bls_cids)?;
    let secp_msg_root = Amt::new_from_slice(data.chain_store.blockstore(), &secp_cids)?;

    let mmcid = data.chain_store.blockstore().put(
        &TxMeta {
            bls_message_root: bls_msg_root,
            secp_message_root: secp_msg_root,
        },
        Blake2b256,
    )?;

    let calculated_bls_agg = if bls_sigs.is_empty() {
        Some(crypto::Signature::new_bls(vec![]))
    } else {
        Some(crypto::Signature::new_bls(
            bls_signatures::aggregate(
                &bls_sigs
                    .iter()
                    .map(|s| s.bytes())
                    .map(bls_signatures::Signature::from_bytes)
                    .collect::<Result<Vec<_>, _>>()?,
            )
            .unwrap()
            .as_bytes(),
        ))
    };
    let pweight = chain::weight(data.chain_store.blockstore(), &pts.as_ref())?;
    let base_fee = chain::compute_base_fee(data.chain_store.blockstore(), &pts.as_ref())?;

    let mut next = BlockHeader::builder()
        .messages(mmcid)
        .bls_aggregate(calculated_bls_agg)
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

    let key = wallet::find_key(&worker, &*data.keystore.as_ref().write().await)?;
    let sig = wallet::sign(
        *key.key_info.key_type(),
        key.key_info.private_key(),
        &next.to_signing_bytes()?,
    )?;
    next.signature = Some(sig);

    Ok(BlockMsgJson(BlockMsg {
        header: next,
        bls_messages: bls_cids,
        secpk_messages: secp_cids,
    }))
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct MarketDeal {
    proposal: DealProposal,
    state: DealState,
}

pub(crate) async fn state_market_deals<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(TipsetKeysJson,)>,
) -> Result<HashMap<String, MarketDeal>, JsonRpcError> {
    let (TipsetKeysJson(tsk),) = params;
    let ts = data.chain_store.tipset_from_keys(&tsk).await?;
    let actor = data
        .state_manager
        .get_actor(market::ADDRESS, ts.parent_state())?
        .ok_or("Power actor address could not be resolved")?;
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

pub(crate) async fn state_miner_sector_allocated<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(AddressJson, u64, TipsetKeysJson)>,
) -> Result<bool, JsonRpcError> {
    let (AddressJson(maddr), sector_num, TipsetKeysJson(tsk)) = params;
    let ts = data.chain_store.tipset_from_keys(&tsk).await?;

    let actor = data
        .state_manager
        .get_actor(&maddr, ts.parent_state())?
        .ok_or(format!("Miner actor {} could not be resolved", maddr))?;
    let allocated_sectors = match miner::State::load(data.state_manager.blockstore(), &actor)? {
        miner::State::V0(m) => data
            .chain_store
            .db
            .get::<bitfield::BitField>(&m.allocated_sectors)?
            .ok_or("allocated sectors bitfield not found")?
            .get(sector_num as usize),
        miner::State::V2(m) => data
            .chain_store
            .db
            .get::<bitfield::BitField>(&m.allocated_sectors)?
            .ok_or("allocated sectors bitfield not found")?
            .get(sector_num as usize),
    };

    Ok(allocated_sectors)
}

pub(crate) async fn state_miner_pre_commit_deposit_for_power<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
    V: ProofVerifier + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(AddressJson, SectorPreCommitInfo, TipsetKeysJson)>,
) -> Result<String, JsonRpcError> {
    let (AddressJson(maddr), pci, TipsetKeysJson(tsk)) = params;
    let ts = data.chain_store.tipset_from_keys(&tsk).await?;
    let (state, _) = data.state_manager.tipset_state::<V>(&ts).await?;
    let state = StateTree::new_from_root(data.chain_store.db.as_ref(), &state)?;
    let ssize = pci.seal_proof.sector_size()?;

    let actor = state
        .get_actor(market::ADDRESS)?
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
    let sector_weight = actor::actorv2::miner::qa_power_for_weight(ssize, duration, &w, &vw);

    let actor = state
        .get_actor(power::ADDRESS)?
        .ok_or("couldnt load power actor")?;
    let power_smoothed =
        power::State::load(data.state_manager.blockstore(), &actor)?.total_power_smoothed();

    let reward_actor = state
        .get_actor(reward::ADDRESS)?
        .ok_or("couldnt load reward actor")?;
    let deposit = reward::State::load(data.state_manager.blockstore(), &reward_actor)?
        .pre_commit_deposit_for_power(power_smoothed, &sector_weight);

    let ret: BigInt = (deposit * 110) / 100;
    Ok(ret.to_string())
}

pub(crate) async fn state_miner_initial_pledge_collateral<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
    V: ProofVerifier + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(AddressJson, SectorPreCommitInfo, TipsetKeysJson)>,
) -> Result<String, JsonRpcError> {
    let (AddressJson(maddr), pci, TipsetKeysJson(tsk)) = params;
    let ts = data.chain_store.tipset_from_keys(&tsk).await?;
    let (state, _) = data.state_manager.tipset_state::<V>(&ts).await?;
    let state = StateTree::new_from_root(data.chain_store.db.as_ref(), &state)?;
    let ssize = pci.seal_proof.sector_size()?;

    let actor = state
        .get_actor(market::ADDRESS)?
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
    let sector_weight = actor::actorv2::miner::qa_power_for_weight(ssize, duration, &w, &vw);

    let actor = state
        .get_actor(power::ADDRESS)?
        .ok_or("couldnt load power actor")?;
    let power_state = power::State::load(data.state_manager.blockstore(), &actor)?;
    let power_smoothed = power_state.total_power_smoothed();
    let total_locked = power_state.total_locked();

    let circ_supply = data
        .state_manager
        .get_circulating_supply(ts.epoch(), &state)?;

    let reward_actor = state
        .get_actor(reward::ADDRESS)?
        .ok_or("couldnt load reward actor")?;

    let initial_pledge = reward::State::load(data.state_manager.blockstore(), &reward_actor)?
        .initial_pledge_for_power(&sector_weight, &total_locked, power_smoothed, &circ_supply);

    let ret: BigInt = (initial_pledge * 110) / 100;
    Ok(ret.to_string())
}

/// returns a state tree given a tipset
async fn state_for_ts<DB, V>(
    state_manager: &Arc<StateManager<DB>>,
    ts: Arc<Tipset>,
) -> Result<StateTree<'_, DB>, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
    V: ProofVerifier + Send + Sync + 'static,
{
    let block_store = state_manager.blockstore();
    let (st, _) = state_manager.tipset_state::<V>(&ts).await?;
    let state_tree = StateTree::new_from_root(block_store, &st)?;
    Ok(state_tree)
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ActorStateJson {
    #[serde(with = "cid::json")]
    code: Cid,
    #[serde(with = "cid::json")]
    head: Cid,
    nonce: u64,
    #[serde(with = "bigint_ser::json")]
    balance: BigInt,
}

impl From<ActorState> for ActorStateJson {
    fn from(a: ActorState) -> Self {
        Self {
            code: a.code,
            head: a.state,
            nonce: a.sequence,
            balance: a.balance,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MiningBaseInfoJson {
    #[serde(with = "bigint_ser::json::opt")]
    pub miner_power: Option<BigInt>,
    #[serde(with = "bigint_ser::json::opt")]
    pub network_power: Option<BigInt>,
    pub sectors: Vec<SectorInfoJson>,
    #[serde(with = "address::json")]
    pub worker_key: Address,
    pub sector_size: SectorSize,
    #[serde(with = "beacon::json")]
    pub prev_beacon_entry: BeaconEntry,
    pub beacon_entries: Vec<BeaconEntryJson>,
    pub eligible_for_mining: bool,
}

impl From<MiningBaseInfo> for MiningBaseInfoJson {
    fn from(info: MiningBaseInfo) -> Self {
        Self {
            miner_power: info.miner_power,
            network_power: info.network_power,
            sectors: info
                .sectors
                .into_iter()
                .map(From::from)
                .collect::<Vec<SectorInfoJson>>(),
            worker_key: info.worker_key,
            sector_size: info.sector_size,
            prev_beacon_entry: info.prev_beacon_entry,
            beacon_entries: info
                .beacon_entries
                .into_iter()
                .map(BeaconEntryJson)
                .collect::<Vec<BeaconEntryJson>>(),
            eligible_for_mining: info.elligable_for_minning,
        }
    }
}
