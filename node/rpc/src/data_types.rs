use async_std::channel::Sender;
use async_std::sync::{Arc, RwLock};
use beacon::{Beacon, BeaconSchedule};
use blocks::Tipset;
use blockstore::BlockStore;
use chain::headchange_json::HeadChangeJson;
use chain::ChainStore;
use chain_sync::{BadBlockCache, SyncState};
use crossbeam::atomic::AtomicCell;
use forest_libp2p::NetworkMessage;
use message_pool::{MessagePool, MpoolRpcProvider};
use serde::Serialize;
use serde::{Deserialize, Deserializer};
use serde_json::{value::RawValue, Value};
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
    pub chain_notify_count: Arc<AtomicCell<usize>>,
}

pub type State<DB, KS, B> = Arc<RpcState<DB, KS, B>>;

// jsonrpc-v2 request object emulation
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct JsonRpcRequestObject {
    #[serde(default = "default_jsonrpc")]
    pub jsonrpc: String,
    pub method: Box<str>,
    pub params: Option<InnerParams>,
    #[serde(deserialize_with = "JsonRpcRequestObject::deserialize_id")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Option<Id>>,
}

fn default_jsonrpc() -> String {
    "2.0".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum InnerParams {
    Value(Value),
    Raw(Box<RawValue>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    Num(i64),
    Str(Box<str>),
    Null,
}

impl JsonRpcRequestObject {
    fn deserialize_id<'de, D>(deserializer: D) -> Result<Option<Option<Id>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Some(Option::deserialize(deserializer)?))
    }
}
