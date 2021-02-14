use async_std::channel::Sender;
use async_std::sync::{Arc, RwLock};
use beacon::{Beacon, BeaconSchedule};
use blocks::Tipset;
use blockstore::BlockStore;
use chain::headchange_json::HeadChangeJson;
use chain::ChainStore;
use chain_sync::{BadBlockCache, SyncState};
use forest_libp2p::NetworkMessage;
use message_pool::{MessagePool, MpoolRpcProvider};
use serde::Serialize;
use state_manager::StateManager;
use wallet::KeyStore;

#[derive(Serialize)]
pub struct StreamingData<'a> {
    json_rpc: &'a str,
    method: &'a str,
    params: (usize, Vec<HeadChangeJson<'a>>),
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
    // TODO in future, these should try to be removed, it currently isn't possible to handle
    // streaming with the current RPC framework. Should be able to just use subscribed channel.
    pub chain_notify_count: Arc<RwLock<usize>>,
}

pub type State<DB, KS, B> = Arc<RpcState<DB, KS, B>>;
