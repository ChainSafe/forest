// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod auth_layer;
mod channel;
mod client;
mod request;

pub use client::Client;
pub use error::ServerError;
use reflect::Ctx;
pub use reflect::{ApiVersion, RpcMethod, RpcMethodExt};
pub use request::Request;
mod error;
mod reflect;
pub mod types;
pub use methods::*;
use reflect::Permission;

/// Protocol or transport-specific error
pub use jsonrpsee::core::ClientError;

#[allow(unused)]
/// All handler definitions.
///
/// Usage guide:
/// ```ignore
/// use crate::rpc::{self, prelude::*};
///
/// let client = rpc::Client::from(..);
/// ChainHead::call(&client, ()).await?;
/// fn foo() -> rpc::ClientError {..}
/// fn bar() -> rpc::ServerError {..}
/// ```
pub mod prelude {
    use super::*;

    pub use reflect::RpcMethodExt as _;

    macro_rules! export {
        ($ty:ty) => {
            pub use $ty;
        };
    }
    auth::for_each_method!(export);
    beacon::for_each_method!(export);
    chain::for_each_method!(export);
    common::for_each_method!(export);
    gas::for_each_method!(export);
    mpool::for_each_method!(export);
    net::for_each_method!(export);
    state::for_each_method!(export);
    node::for_each_method!(export);
    sync::for_each_method!(export);
    wallet::for_each_method!(export);
    eth::for_each_method!(export);
}

/// All the methods live in their own folder
///
/// # Handling types
/// - If a `struct` or `enum` is only used in the RPC API, it should live in `src/rpc`.
///   - If it is used in only one API vertical (i.e `auth` or `chain`), then it should live
///     in either:
///     - `src/rpc/methods/auth.rs` (if there are only a few).
///     - `src/rpc/methods/auth/types.rs` (if there are so many that they would cause clutter).
///   - If it is used _across_ API verticals, it should live in `src/rpc/types.rs`
///
/// # Interactions with the [`lotus_json`] APIs
/// - Types may have fields which must go through [`LotusJson`],
///   and MUST reflect that in their [`JsonSchema`].
///   You have two options for this:
///   - Use `#[attributes]` to control serialization and schema generation:
///     ```ignore
///     #[derive(Deserialize, Serialize, JsonSchema)]
///     struct Foo {
///         #[serde(with = "crate::lotus_json")] // perform the conversion
///         #[schemars(with = "LotusJson<Cid>")] // advertise the schema to be converted
///         cid: Cid, // use the native type in application logic
///     }
///     ```
///   - Use [`LotusJson`] directly. This means that serialization and the [`JsonSchema`]
///     will never go out of sync.
///     ```ignore
///     #[derive(Deserialize, Serialize, JsonSchema)]
///     struct Foo {
///         cid: LotusJson<Cid>, // use the shim type in application logic, manually performing conversions
///     }
///     ```
///
/// # `for_each_method`
/// Each API vertical exposes a [`for_each_method!`](auth::for_each_method) macro,
/// which is used in three places:
/// - [`prelude`], where all the methods are exported for use in the codebase.
/// - [`auth_layer`], where their [`RpcMethod::PERMISSION`]s are registered.
/// - [`create_module`], where they're actually registered to be served.
///
/// [`lotus_json`]: crate::lotus_json
/// [`HasLotusJson`]: crate::lotus_json::HasLotusJson
/// [`LotusJson`]: crate::lotus_json::LotusJson
/// [`JsonSchema`]: schemars::JsonSchema
mod methods {
    pub mod auth;
    pub mod beacon;
    pub mod chain;
    pub mod common;
    pub mod eth;
    pub mod gas;
    pub mod mpool;
    pub mod net;
    pub mod node;
    pub mod state;
    pub mod sync;
    pub mod wallet;
}

use crate::key_management::KeyStore;
use crate::rpc::auth_layer::AuthLayer;
use crate::rpc::channel::RpcModule as FilRpcModule;
pub use crate::rpc::channel::CANCEL_METHOD_NAME;

use crate::blocks::Tipset;
use fvm_ipld_blockstore::Blockstore;
use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use jsonrpsee::{
    server::{stop_channel, RpcModule, RpcServiceBuilder, Server, StopHandle, TowerServiceBuilder},
    Methods,
};
use once_cell::sync::Lazy;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, RwLock};
use tower::Service;
use tracing::info;

use self::reflect::openrpc_types::ParamStructure;

pub const DEFAULT_PORT: u16 = 2345;

/// Request timeout read from environment variables
static DEFAULT_REQUEST_TIMEOUT: Lazy<Duration> = Lazy::new(|| {
    env::var("FOREST_RPC_DEFAULT_TIMEOUT")
        .ok()
        .and_then(|it| Duration::from_secs(it.parse().ok()?).into())
        .unwrap_or(Duration::from_secs(60))
});

const MAX_REQUEST_BODY_SIZE: u32 = 64 * 1024 * 1024;
const MAX_RESPONSE_BODY_SIZE: u32 = MAX_REQUEST_BODY_SIZE;

/// This is where you store persistent data, or at least access to stateful
/// data.
pub struct RPCState<DB> {
    pub keystore: Arc<RwLock<KeyStore>>,
    pub chain_store: Arc<crate::chain::ChainStore<DB>>,
    pub state_manager: Arc<crate::state_manager::StateManager<DB>>,
    pub mpool: Arc<crate::message_pool::MessagePool<crate::message_pool::MpoolRpcProvider<DB>>>,
    pub bad_blocks: Arc<crate::chain_sync::BadBlockCache>,
    pub sync_state: Arc<parking_lot::RwLock<crate::chain_sync::SyncState>>,
    pub network_send: flume::Sender<crate::libp2p::NetworkMessage>,
    pub network_name: String,
    pub tipset_send: flume::Sender<Arc<Tipset>>,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub beacon: Arc<crate::beacon::BeaconSchedule>,
    pub shutdown: mpsc::Sender<()>,
}

impl<DB: Blockstore> RPCState<DB> {
    pub fn store(&self) -> &DB {
        self.chain_store.blockstore()
    }
}

#[derive(Clone)]
struct PerConnection<RpcMiddleware, HttpMiddleware> {
    methods: Methods,
    stop_handle: StopHandle,
    svc_builder: TowerServiceBuilder<RpcMiddleware, HttpMiddleware>,
    keystore: Arc<RwLock<KeyStore>>,
}

pub async fn start_rpc<DB>(state: RPCState<DB>, rpc_endpoint: SocketAddr) -> anyhow::Result<()>
where
    DB: Blockstore + Send + Sync + 'static,
{
    // `Arc` is needed because we will share the state between two modules
    let state = Arc::new(state);
    let keystore = state.keystore.clone();
    let (mut module, _schema) = create_module(state.clone());

    let mut pubsub_module = FilRpcModule::default();

    pubsub_module.register_channel("Filecoin.ChainNotify", {
        let state_clone = state.clone();
        move |params| chain::chain_notify(params, &state_clone)
    })?;
    module.merge(pubsub_module)?;

    let (stop_handle, _handle) = stop_channel();

    let per_conn = PerConnection {
        methods: module.into(),
        stop_handle: stop_handle.clone(),
        svc_builder: Server::builder()
            // Default size (10 MiB) is not enough for methods like `Filecoin.StateMinerActiveSectors`
            .max_request_body_size(MAX_REQUEST_BODY_SIZE)
            .max_response_body_size(MAX_RESPONSE_BODY_SIZE)
            .to_service_builder(),
        keystore,
    };

    let make_service = make_service_fn(move |_conn: &AddrStream| {
        let per_conn = per_conn.clone();

        async move {
            anyhow::Ok(service_fn(move |req| {
                let PerConnection {
                    methods,
                    stop_handle,
                    svc_builder,
                    keystore,
                } = per_conn.clone();

                let headers = req.headers().clone();
                let rpc_middleware = RpcServiceBuilder::new().layer(AuthLayer {
                    headers,
                    keystore: keystore.clone(),
                });

                let mut svc = svc_builder
                    .set_rpc_middleware(rpc_middleware)
                    .build(methods, stop_handle);

                async move { svc.call(req).await }
            }))
        }
    });

    info!("Ready for RPC connections");
    hyper::Server::bind(&rpc_endpoint)
        .serve(make_service)
        .await?;

    info!("Stopped accepting RPC connections");

    Ok(())
}

fn create_module<DB>(
    state: Arc<RPCState<DB>>,
) -> (RpcModule<RPCState<DB>>, reflect::openrpc_types::OpenRPC)
where
    DB: Blockstore + Send + Sync + 'static,
{
    let mut module = reflect::SelfDescribingRpcModule::new(state, ParamStructure::ByPosition);
    macro_rules! register {
        ($ty:ty) => {
            <$ty>::register(&mut module);
        };
    }
    auth::for_each_method!(register);
    beacon::for_each_method!(register);
    chain::for_each_method!(register);
    common::for_each_method!(register);
    gas::for_each_method!(register);
    mpool::for_each_method!(register);
    net::for_each_method!(register);
    state::for_each_method!(register);
    node::for_each_method!(register);
    sync::for_each_method!(register);
    wallet::for_each_method!(register);
    eth::for_each_method!(register);
    module.finish()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::task::JoinSet;

    use crate::{
        blocks::Chain4U,
        chain::ChainStore,
        chain_sync::SyncConfig,
        db::car::PlainCar,
        genesis::get_network_name_from_genesis,
        message_pool::{MessagePool, MpoolRpcProvider},
        networks::ChainConfig,
        state_manager::StateManager,
        KeyStoreConfig,
    };

    use super::*;

    // TODO(forest): https://github.com/ChainSafe/forest/issues/4047
    //               `tokio` shouldn't be necessary
    // `cargo test --lib -- --exact 'rpc::tests::openrpc'`
    // `cargo insta review`
    #[tokio::test]
    async fn openrpc() {
        let (_, spec) = create_module(Arc::new(RPCState::calibnet()));
        insta::assert_yaml_snapshot!(spec);
    }

    impl RPCState<Chain4U<PlainCar<&'static [u8]>>> {
        pub fn calibnet() -> Self {
            let chain_store = Arc::new(ChainStore::calibnet());
            let genesis = chain_store.genesis_block_header();
            let state_manager = Arc::new(
                StateManager::new(
                    chain_store.clone(),
                    Arc::new(ChainConfig::calibnet()),
                    Arc::new(SyncConfig::default()),
                )
                .unwrap(),
            );
            let beacon = Arc::new(
                state_manager
                    .chain_config()
                    .get_beacon_schedule(genesis.timestamp),
            );
            let (network_send, _) = flume::bounded(0);
            let (tipset_send, _) = flume::bounded(1);
            let network_name = get_network_name_from_genesis(genesis, &state_manager).unwrap();
            let message_pool = MessagePool::new(
                MpoolRpcProvider::new(chain_store.publisher().clone(), state_manager.clone()),
                network_name.clone(),
                network_send.clone(),
                Default::default(),
                state_manager.chain_config().clone(),
                &mut JoinSet::default(),
            )
            .unwrap();
            RPCState {
                state_manager,
                keystore: Arc::new(RwLock::new(KeyStore::new(KeyStoreConfig::Memory).unwrap())),
                mpool: Arc::new(message_pool),
                bad_blocks: Default::default(),
                sync_state: Default::default(),
                network_send,
                network_name,
                start_time: Default::default(),
                chain_store,
                beacon,
                shutdown: mpsc::channel(1).0, // dummy for tests
                tipset_send,
            }
        }
    }
}
