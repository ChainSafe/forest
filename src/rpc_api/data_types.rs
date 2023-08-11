// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::beacon::BeaconSchedule;
use crate::blocks::{tipset_keys_json::TipsetKeysJson, Tipset};
use crate::chain::ChainStore;
use crate::chain_sync::{BadBlockCache, SyncState};
use crate::ipld::json::IpldJson;
use crate::json::{cid::CidJson, message_receipt::json::ReceiptJson, token_amount::json};
use crate::key_management::KeyStore;
pub use crate::libp2p::{Multiaddr, Protocol};
use crate::libp2p::{Multihash, NetworkMessage};
use crate::message::signed_message::SignedMessage;
use crate::message_pool::{MessagePool, MpoolRpcProvider};
use crate::shim::{econ::TokenAmount, message::Message};
use crate::state_manager::StateManager;
use ahash::HashSet;
use chrono::Utc;
use cid::Cid;
use fil_actor_interface::market::{DealProposal, DealState};
use fvm_ipld_blockstore::Blockstore;
use jsonrpc_v2::{MapRouter as JsonRpcMapRouter, Server as JsonRpcServer};
use parking_lot::RwLock as SyncRwLock;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// This is where you store persistent data, or at least access to stateful
/// data.
pub struct RPCState<DB>
where
    DB: Blockstore,
{
    pub keystore: Arc<RwLock<KeyStore>>,
    pub chain_store: Arc<ChainStore<DB>>,
    pub state_manager: Arc<StateManager<DB>>,
    pub mpool: Arc<MessagePool<MpoolRpcProvider<DB>>>,
    pub bad_blocks: Arc<BadBlockCache>,
    pub sync_state: Arc<SyncRwLock<SyncState>>,
    pub network_send: flume::Sender<NetworkMessage>,
    pub network_name: String,
    pub start_time: chrono::DateTime<Utc>,
    pub new_mined_block_tx: flume::Sender<Arc<Tipset>>,
    pub beacon: Arc<BeaconSchedule>,
    pub gc_event_tx: flume::Sender<flume::Sender<anyhow::Result<()>>>,
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
    #[serde(rename = "BlsMessages", with = "crate::json::message::json::vec")]
    pub bls_msg: Vec<Message>,
    #[serde(
        rename = "SecpkMessages",
        with = "crate::json::signed_message::json::vec"
    )]
    pub secp_msg: Vec<SignedMessage>,
    #[serde(rename = "Cids", with = "crate::json::cid::vec")]
    pub cids: Vec<Cid>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MessageSendSpec {
    #[serde(with = "json")]
    max_fee: TokenAmount,
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

// Net API
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AddrInfo {
    #[serde(rename = "ID")]
    pub id: String,
    pub addrs: HashSet<Multiaddr>,
}

#[derive(Serialize, Deserialize)]
pub struct PeerID {
    pub multihash: Multihash,
}

/// Represents the current version of the API.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct APIVersion {
    pub version: String,
    pub api_version: Version,
    pub block_delay: u64,
}

/// Integer based value on version information. Highest order bits for Major,
/// Mid order for Minor and lowest for Patch.
#[derive(Serialize, Deserialize)]
pub struct Version(u32);

impl Version {
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self((major as u32) << 16 | (minor as u32) << 8 | (patch as u32))
    }
}
