// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use forest_actor_interface::market::{DealProposal, DealState};
use forest_beacon::{json::BeaconEntryJson, Beacon, BeaconSchedule};
use forest_blocks::{
    election_proof::json::ElectionProofJson, ticket::json::TicketJson,
    tipset_keys_json::TipsetKeysJson, Tipset,
};
use forest_chain::{headchange_json::SubscriptionHeadChange, ChainStore};
use forest_chain_sync::{BadBlockCache, SyncState};
use forest_ipld::json::IpldJson;
use forest_json::address::json::AddressJson;
use forest_json::cid::CidJson;
use forest_json::message_receipt::json::ReceiptJson;
use forest_json::sector::json::PoStProofJson;
use forest_json::signed_message::json::SignedMessageJson;
use forest_json::token_amount::json;
use forest_key_management::KeyStore;
pub use forest_libp2p::{Multiaddr, Protocol};
use forest_libp2p::{Multihash, NetworkMessage};
use forest_message::signed_message::SignedMessage;
use forest_message_pool::{MessagePool, MpoolRpcProvider};
use forest_state_manager::StateManager;
use fvm::state_tree::ActorState;
use fvm_ipld_bitfield::json::BitFieldJson;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::address::Address;
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
    DB: Blockstore,
    B: Beacon,
{
    pub keystore: Arc<RwLock<KeyStore>>,
    pub chain_store: Arc<ChainStore<DB>>,
    pub state_manager: Arc<StateManager<DB>>,
    pub mpool: Arc<MessagePool<MpoolRpcProvider<DB>>>,
    pub bad_blocks: Arc<BadBlockCache>,
    pub sync_state: Arc<RwLock<SyncState>>,
    pub network_send: flume::Sender<NetworkMessage>,
    pub network_name: String,
    pub new_mined_block_tx: flume::Sender<Arc<Tipset>>,
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
    #[serde(rename = "BlsMessages", with = "forest_json::message::json::vec")]
    pub bls_msg: Vec<Message>,
    #[serde(
        rename = "SecpkMessages",
        with = "forest_json::signed_message::json::vec"
    )]
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
    balance: TokenAmount,
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
    pub receipt: ReceiptJson,
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

/// Represents the current version of the API.
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct APIVersion {
    pub version: String,
    pub api_version: Version,
    pub block_delay: u64,
}

/// Integer based value on version information. Highest order bits for Major, Mid order for Minor
/// and lowest for Patch.
#[derive(Serialize)]
pub struct Version(u32);

impl Version {
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self((major as u32) << 16 | (minor as u32) << 8 | (patch as u32))
    }
}
