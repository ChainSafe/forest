// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::channel::Sender;
use cid::Cid;
use forest_actor_interface::market::{DealProposal, DealState};
use forest_beacon::BeaconEntry;
use forest_beacon::{json::BeaconEntryJson, Beacon, BeaconSchedule};
use forest_blocks::{
    election_proof::json::ElectionProofJson, ticket::json::TicketJson,
    tipset_keys_json::TipsetKeysJson, Tipset,
};
use forest_chain::{headchange_json::SubscriptionHeadChange, ChainStore};
use forest_chain_sync::{BadBlockCache, SyncState};
use forest_fil_types::SectorSize;
use forest_fil_types::{json::SectorInfoJson, sector::post::json::PoStProofJson};
use forest_ipld::json::IpldJson;
use forest_ipld_blockstore::BlockStore;
use forest_json::address::json::AddressJson;
use forest_json::bigint::json;
use forest_json::cid::CidJson;
use forest_key_management::KeyStore;
pub use forest_libp2p::{Multiaddr, Protocol};
use forest_libp2p::{Multihash, NetworkMessage};
use forest_message::{
    message_receipt::json::MessageReceiptJson, signed_message,
    signed_message::json::SignedMessageJson, SignedMessage,
};
use forest_message_pool::{MessagePool, MpoolRpcProvider};
use forest_state_manager::{MiningBaseInfo, StateManager};
use fvm::state_tree::ActorState;
use fvm_ipld_bitfield::json::BitFieldJson;
use fvm_shared::address::Address;
use fvm_shared::bigint::BigInt;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::econ::TokenAmount;
use fvm_shared::message::Message;
use jsonrpc_v2::{MapRouter as JsonRpcMapRouter, Server as JsonRpcServer};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

// RPC State
#[derive(Serialize)]
pub struct StreamingData<'a> {
    pub json_rpc: &'a str,
    pub method: &'a str,
    pub params: SubscriptionHeadChange,
}

/// This is where you store persistent data, or at least access to stateful data.
pub struct RPCState<DB, B>
where
    DB: BlockStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    pub keystore: Arc<RwLock<KeyStore>>,
    pub chain_store: Arc<ChainStore<DB>>,
    pub state_manager: Arc<StateManager<DB>>,
    pub mpool: Arc<MessagePool<MpoolRpcProvider<DB>>>,
    pub bad_blocks: Arc<BadBlockCache>,
    pub sync_state: Arc<RwLock<SyncState>>,
    pub network_send: Sender<NetworkMessage>,
    pub network_name: String,
    pub new_mined_block_tx: Sender<Arc<Tipset>>,
    pub beacon: Arc<BeaconSchedule<B>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RPCSyncState {
    #[serde(rename = "ActiveSyncs")]
    pub active_syncs: Vec<SyncState>,
}

pub type JsonRpcServerState = Arc<JsonRpcServer<JsonRpcMapRouter>>;

// Chain API
#[derive(Serialize, Deserialize)]
pub struct BlockMessages {
    #[serde(rename = "BlsMessages", with = "forest_message::message::json::vec")]
    pub bls_msg: Vec<Message>,
    #[serde(rename = "SecpkMessages", with = "signed_message::json::vec")]
    pub secp_msg: Vec<SignedMessage>,
    #[serde(rename = "Cids", with = "forest_json::cid::vec")]
    pub cids: Vec<Cid>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MessageSendSpec {
    #[serde(with = "json")]
    max_fee: TokenAmount,
}

// State API
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Deadline {
    pub post_submissions: BitFieldJson,
    pub disputable_proof_count: usize,
}

#[derive(Serialize)]
pub struct Fault {
    miner: Address,
    epoch: ChainEpoch,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Partition {
    pub all_sectors: BitFieldJson,
    pub faulty_sectors: BitFieldJson,
    pub recovering_sectors: BitFieldJson,
    pub live_sectors: BitFieldJson,
    pub active_sectors: BitFieldJson,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ActorStateJson {
    #[serde(with = "forest_json::cid")]
    code: Cid,
    #[serde(with = "forest_json::cid")]
    head: Cid,
    nonce: u64,
    #[serde(with = "json")]
    balance: BigInt,
}

impl ActorStateJson {
    pub fn nonce(&self) -> u64 {
        self.nonce
    }
}

impl From<ActorStateJson> for ActorState {
    fn from(a: ActorStateJson) -> Self {
        Self {
            code: a.code,
            state: a.head,
            sequence: a.nonce,
            balance: a.balance,
        }
    }
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
pub struct MarketDeal {
    pub proposal: DealProposal,
    pub state: DealState,
}

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

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BlockTemplate {
    pub miner: AddressJson,
    pub parents: TipsetKeysJson,
    pub ticket: TicketJson,
    pub eproof: ElectionProofJson,
    pub beacon_values: Vec<BeaconEntryJson>,
    pub messages: Vec<SignedMessageJson>,
    pub epoch: i64,
    pub timestamp: u64,
    #[serde(rename = "WinningPoStProof")]
    pub winning_post_proof: Vec<PoStProofJson>,
}
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MiningBaseInfoJson {
    #[serde(with = "json::option")]
    pub miner_power: Option<TokenAmount>,
    #[serde(with = "json::option")]
    pub network_power: Option<TokenAmount>,
    pub sectors: Vec<SectorInfoJson>,
    #[serde(with = "forest_json::address::json")]
    pub worker_key: Address,
    pub sector_size: SectorSize,
    #[serde(with = "forest_beacon::json")]
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
            eligible_for_mining: info.eligible_for_mining,
        }
    }
}

// Net API
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AddrInfo {
    #[serde(rename = "ID")]
    pub id: String,
    pub addrs: Vec<Multiaddr>,
}

#[derive(Serialize, Deserialize)]
pub struct PeerID {
    pub multihash: Multihash,
}
