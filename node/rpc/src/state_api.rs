// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::RpcState;
use actor::miner::MinerInfo;
use actor::miner::{
    ChainSectorInfo, Deadlines, Fault, SectorOnChainInfo, SectorPreCommitOnChainInfo, State,
    WorkerKeyChange,
};
use address::{json::AddressJson, Address};
use async_std::task;
use beacon::{json::BeaconEntryJson, Beacon, BeaconEntry};
use bitfield::json::BitFieldJson;
use blocks::tipset_keys_json::TipsetKeysJson;
use blocks::{tipset_json::TipsetJson, Tipset};
use blockstore::BlockStore;
use cid::{json::CidJson, Cid};
use clock::{ChainEpoch, EPOCH_UNDEFINED};
use encoding::BytesDe;
use fil_types::json::SectorInfoJson;
use fil_types::{
    deadlines::DeadlineInfo,
    use_newest_network,
    verifier::{FullVerifier, ProofVerifier},
    NetworkVersion, RegisteredSealProof, SectorNumber, SectorSize, NEWEST_NETWORK_VERSION,
    UPGRADE_BREEZE_HEIGHT, UPGRADE_SMOKE_HEIGHT,
};
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use libp2p::core::PeerId;
use message::{
    message_receipt::json::MessageReceiptJson,
    unsigned_message::{json::UnsignedMessageJson, UnsignedMessage},
};
use num_bigint::{bigint_ser, BigInt};
use serde::Serialize;
use state_manager::MiningBaseInfo;
use state_manager::{InvocResult, MarketBalance, StateManager};
use state_tree::StateTree;
use std::convert::{TryFrom, TryInto};
use std::sync::Arc;
use wallet::KeyStore;

// TODO handle using configurable verification implementation in RPC (all defaulting to Full).

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MessageLookup {
    pub receipt: MessageReceiptJson,
    pub tipset: TipsetJson,
}

/// returns info about the given miner's sectors. If the filter bitfield is nil, all sectors are included.
/// If the filterOut boolean is set to true, any sectors in the filter are excluded.
/// If false, only those sectors in the filter are included.
pub(crate) async fn state_miner_sector<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(AddressJson, BitFieldJson, bool, TipsetKeysJson)>,
) -> Result<Vec<ChainSectorInfo>, JsonRpcError> {
    let (address, filter, filter_out, key) = params;
    let mut bitfield_filter = filter.into();
    let address = address.into();
    let state_manager = &data.state_manager;
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    let mut filter = Some(&mut bitfield_filter);
    state_manager
        .get_miner_sector_set::<FullVerifier>(&tipset, &address, &mut filter, filter_out)
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

/// returns all the proving deadlines for the given miner
pub(crate) async fn state_miner_deadlines<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(AddressJson, TipsetKeysJson)>,
) -> Result<Deadlines, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (actor, key) = params;
    let actor = actor.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    state_manager
        .get_miner_deadlines::<FullVerifier>(&tipset, &actor)
        .map_err(|e| e.into())
}

/// returns the PreCommit info for the specified miner's sector
pub(crate) async fn state_sector_precommit_info<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(AddressJson, SectorNumber, TipsetKeysJson)>,
) -> Result<SectorPreCommitOnChainInfo, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (address, sector_number, key) = params;
    let address = address.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
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
) -> Result<MinerInfoJson, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (actor, key) = params;
    let actor = actor.into();
    let miner_state = data
        .state_manager
        .chain_store()
        .miner_load_actor_tsk(&actor, &key.into())
        .await
        .map_err(|e| format!("Could not load miner {:?}", e))?;
    let miner_info = miner_state
        .get_info(state_manager.blockstore())
        .map_err(|e| format!("Could not get info {:?}", e))?;
    Ok(miner_info.try_into()?)
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
        .miner_sector_info::<FullVerifier>(&address, &sector_number, &tipset)
        .map_err(|e| e.into())
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
    let (actor, key) = params;
    let actor = actor.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    let miner_actor_state: State =
        state_manager.load_actor_state(&actor, &tipset.parent_state())?;

    Ok(miner_actor_state
        .deadline_info(tipset.epoch())
        .next_not_elapsed())
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

pub(crate) async fn state_get_network_version(
    Params(params): Params<ChainEpoch>,
) -> Result<NetworkVersion, JsonRpcError> {
    let height = params;
    if use_newest_network() {
        return Ok(NEWEST_NETWORK_VERSION);
    }
    if height <= UPGRADE_BREEZE_HEIGHT {
        return Ok(NEWEST_NETWORK_VERSION);
    }

    if height <= UPGRADE_SMOKE_HEIGHT {
        return Ok(NetworkVersion::V0);
    }

    Ok(NEWEST_NETWORK_VERSION)
}

/// returns the indicated actor's nonce and balance.
pub(crate) async fn state_get_actor<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
>(
    data: Data<RpcState<DB, KS, B>>,
    Params(params): Params<(AddressJson, TipsetKeysJson)>,
) -> Result<Option<actor::ActorState>, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (actor, key) = params;
    let actor = actor.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    let state = state_for_ts(&state_manager, tipset).await?;
    state.get_actor(&actor).map_err(|e| e.into())
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
    let state = state_for_ts(&state_manager, tipset).await?;
    let address = interpreter::resolve_to_key_addr(&state, state_manager.blockstore(), &actor)?;
    Ok(Some(address.into()))
}
/// retrieves the ID address of the given address
pub(crate) async fn state_lookup_id<
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
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
    let state = state_for_ts(&state_manager, tipset).await?;
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
    let (tipset, receipt) = state_manager
        .wait_for_message(state_manager.get_subscriber(), &cid, confidence)
        .await?;
    let tipset = tipset.ok_or("wait for msg returned empty tuple")?;
    let receipt = receipt.ok_or("wait for msg returned empty receipt")?;
    let tipset_json: TipsetJson = tipset.into();
    Ok(MessageLookup {
        receipt: receipt.into(),
        tipset: tipset_json,
    })
}

/// returns a state tree given a tipset
async fn state_for_ts<DB>(
    state_manager: &Arc<StateManager<DB>>,
    ts: Arc<Tipset>,
) -> Result<StateTree<'_, DB>, JsonRpcError>
where
    DB: BlockStore + Send + Sync + 'static,
{
    let block_store = state_manager.blockstore();
    let (st, _) = task::block_on(state_manager.tipset_state::<FullVerifier>(&ts))?;
    let state_tree = StateTree::new_from_root(block_store, &st)?;
    Ok(state_tree)
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

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MinerInfoJson {
    owner: AddressJson,
    worker: AddressJson,
    #[serde(with = "address::json::opt")]
    new_worker: Option<Address>,
    #[serde(with = "address::json::vec")]
    control_addresses: Vec<Address>,
    worker_change_epoch: ChainEpoch,
    pending_worker_key: Option<WorkerKeyChange>,
    peer_id: String,
    multi_address: Vec<BytesDe>,
    seal_proof_type: RegisteredSealProof,
    sector_size: SectorSize,
    window_post_partition_sectors: u64,
    consensus_fault_elapsed: ChainEpoch,
}

impl TryFrom<MinerInfo> for MinerInfoJson {
    type Error = String;

    fn try_from(info: MinerInfo) -> Result<Self, Self::Error> {
        Ok(Self {
            owner: info.owner.into(),
            worker: info.worker.into(),
            new_worker: None,
            control_addresses: info.control_addresses,
            worker_change_epoch: EPOCH_UNDEFINED,
            pending_worker_key: info.pending_worker_key,
            peer_id: PeerId::from_bytes(info.peer_id)
                .map_err(|e| format!("Could not convert bytes {:?} into PeerId", e))?
                .to_string(),
            multi_address: info.multi_address,
            seal_proof_type: info.seal_proof_type,
            sector_size: info.sector_size,
            window_post_partition_sectors: info.window_post_partition_sectors,
            consensus_fault_elapsed: EPOCH_UNDEFINED,
        })
    }
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
