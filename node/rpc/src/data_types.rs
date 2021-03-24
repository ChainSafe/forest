// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use async_std::channel::Sender;
use async_std::sync::{Arc, RwLock};
use jsonrpc_v2::{MapRouter as JsonRpcMapRouter, Server as JsonRpcServer};
use serde::Serialize;

use beacon::{Beacon, BeaconSchedule};
use blocks::Tipset;
use blockstore::BlockStore;
use chain::headchange_json::HeadChangeJson;
use chain::ChainStore;
use chain_sync::{BadBlockCache, SyncState};
use forest_libp2p::NetworkMessage;
use message_pool::{MessagePool, MpoolRpcProvider};
use state_manager::StateManager;
use wallet::KeyStore;

pub type SubscriptionHeadChange = (i64, Vec<HeadChangeJson>);

#[derive(Serialize)]
pub struct StreamingData<'a> {
    pub json_rpc: &'a str,
    pub method: &'a str,
    pub params: SubscriptionHeadChange,
}

/// This is where you store persistent data, or at least access to stateful data.
pub struct RpcState<DB, KS, B>
where
    DB: BlockStore + Send + Sync + 'static,
    KS: KeyStore + Send + Sync + 'static,
    B: Beacon + Send + Sync + 'static,
{
    pub keystore: Arc<RwLock<KS>>,
    pub chain_store: Arc<ChainStore<DB>>,
    pub state_manager: Arc<StateManager<DB>>,
    pub mpool: Arc<MessagePool<MpoolRpcProvider<DB>>>,
    pub bad_blocks: Arc<BadBlockCache>,
    pub sync_state: Arc<RwLock<Vec<Arc<RwLock<SyncState>>>>>,
    pub network_send: Sender<NetworkMessage>,
    pub new_mined_block_tx: Sender<Arc<Tipset>>,
    pub network_name: String,
    pub beacon: Arc<BeaconSchedule<B>>,
}

pub type JsonRpcServerState = Arc<JsonRpcServer<JsonRpcMapRouter>>;
