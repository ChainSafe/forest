// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod auth_layer;
mod channel;
mod client;

pub use client::Client;
pub use error::ServerError;
use methods::chain::new_heads;
use reflect::Ctx;
pub use reflect::{ApiVersion, RpcMethod, RpcMethodExt};
mod error;
mod reflect;
pub mod types;
pub use methods::*;

/// Protocol or transport-specific error
#[allow(unused)]
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
/// - Types defined in the module will only ever be deserialized as JSON, so there
///   will NEVER be a need to implement [`HasLotusJson`] for them.
/// - Types may have fields which must go through [`LotusJson`],
///   and must reflect that in their [`JsonSchema`].
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

use std::net::SocketAddr;
use std::sync::Arc;

use crate::key_management::KeyStore;
use crate::rpc::auth_layer::AuthLayer;
use crate::rpc::channel::RpcModule as FilRpcModule;
pub use crate::rpc::channel::CANCEL_METHOD_NAME;
use crate::rpc::state::*;

use ethereum_types::H256;
use fvm_ipld_blockstore::Blockstore;
use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use jsonrpsee::{
    core::{traits::IdProvider, RegisterMethodError},
    server::{stop_channel, RpcModule, RpcServiceBuilder, Server, StopHandle, TowerServiceBuilder},
    types::SubscriptionId,
    Methods,
};
use rand::Rng;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{mpsc, RwLock};
use tower::Service;
use tracing::info;

use self::reflect::openrpc_types::ParamStructure;

const MAX_REQUEST_BODY_SIZE: u32 = 64 * 1024 * 1024;
const MAX_RESPONSE_BODY_SIZE: u32 = MAX_REQUEST_BODY_SIZE;

const ETH_SUBSCRIPTION: &str = "eth_subscription";

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

#[derive(Debug, Copy, Clone)]
pub struct RandomHexStringIdProvider {}

impl RandomHexStringIdProvider {
    pub fn new() -> Self {
        Self {}
    }
}

impl IdProvider for RandomHexStringIdProvider {
    fn next_id(&self) -> SubscriptionId<'static> {
        let mut bytes = [0u8; 32];
        let mut rng = rand::thread_rng();
        rng.fill(&mut bytes);

        SubscriptionId::Str(format!("{:#x}", H256::from(bytes)).into())
    }
}

pub async fn start_rpc<DB>(state: RPCState<DB>, rpc_endpoint: SocketAddr) -> anyhow::Result<()>
where
    DB: Blockstore + Send + Sync + 'static,
{
    // `Arc` is needed because we will share the state between two modules
    let state = Arc::new(state);
    let keystore = state.keystore.clone();
    let (mut module, _schema) = create_module(state.clone());

    // TODO(forest): https://github.com/ChainSafe/forest/issues/4032
    #[allow(deprecated)]
    register_methods(&mut module)?;

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
            .set_id_provider(RandomHexStringIdProvider::new())
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

#[deprecated = "methods should use `create_module`"]
fn register_methods<DB>(module: &mut RpcModule<RPCState<DB>>) -> Result<(), RegisterMethodError>
where
    DB: Blockstore + Send + Sync + 'static,
{
    use eth::*;
    use gas::*;

    // State API
    module.register_async_method(STATE_NETWORK_VERSION, state_get_network_version::<DB>)?;
    module.register_async_method(STATE_MARKET_BALANCE, state_market_balance::<DB>)?;
    module.register_async_method(STATE_MARKET_DEALS, state_market_deals::<DB>)?;
    module.register_async_method(
        STATE_DEAL_PROVIDER_COLLATERAL_BOUNDS,
        state_deal_provider_collateral_bounds::<DB>,
    )?;
    module.register_async_method(STATE_WAIT_MSG, state_wait_msg::<DB>)?;
    module.register_async_method(STATE_SEARCH_MSG, state_search_msg::<DB>)?;
    module.register_async_method(STATE_SEARCH_MSG_LIMITED, state_search_msg_limited::<DB>)?;
    module.register_async_method(STATE_FETCH_ROOT, state_fetch_root::<DB>)?;
    module.register_async_method(STATE_MARKET_STORAGE_DEAL, state_market_storage_deal::<DB>)?;
    // Gas API
    module.register_async_method(GAS_ESTIMATE_FEE_CAP, gas_estimate_fee_cap::<DB>)?;
    module.register_async_method(GAS_ESTIMATE_GAS_PREMIUM, gas_estimate_gas_premium::<DB>)?;
    // Eth API
    module.register_async_method(ETH_ACCOUNTS, |_, _| eth_accounts())?;
    module.register_async_method(ETH_BLOCK_NUMBER, |_, state| eth_block_number::<DB>(state))?;
    module.register_async_method(ETH_CHAIN_ID, |_, state| eth_chain_id::<DB>(state))?;
    module.register_async_method(ETH_GAS_PRICE, |_, state| eth_gas_price::<DB>(state))?;
    module.register_async_method(ETH_GET_BALANCE, eth_get_balance::<DB>)?;
    module.register_async_method(ETH_GET_BLOCK_BY_NUMBER, eth_get_block_by_number::<DB>)?;
    module.register_method(WEB3_CLIENT_VERSION, move |_, _| {
        crate::utils::version::FOREST_VERSION_STRING.clone()
    })?;
    module.register_subscription(
        ETH_SUBSCRIBE,
        ETH_SUBSCRIPTION,
        ETH_UNSUBSCRIBE,
        |params, pending, ctx| async move {
            let event_types = match params.parse::<Vec<String>>() {
                Ok(v) => v,
                Err(e) => {
                    pending
                        .reject(jsonrpsee::types::ErrorObjectOwned::from(e))
                        .await;
                    // If the subscription has not been "accepted" then
                    // the return value will be "ignored" as it's not
                    // allowed to send out any further notifications on
                    // on the subscription.
                    return Ok(());
                }
            };
            // `event_types` is one OR more of:
            //  - "newHeads": notify when new blocks arrive
            //  - "pendingTransactions": notify when new messages arrive in the message pool
            //  - "logs": notify new event logs that match a criteria

            tracing::trace!("Subscribing to events: {:?}", event_types);

            let mut receiver = new_heads(&ctx);
            tokio::spawn(async move {
                // Mark the subscription is accepted after the params has been parsed successful.
                // This is actually responds the underlying RPC method call and may fail if the
                // connection is closed.
                let sink = pending.accept().await.unwrap();

                tracing::trace!("Subscription started (id: {:?})", sink.subscription_id());

                loop {
                    tokio::select! {
                        action = receiver.recv() => {
                            match action {
                                Ok(v) => {
                                    match jsonrpsee::SubscriptionMessage::from_json(&v) {
                                        Ok(msg) => {
                                            // This fails only if the connection is closed
                                            if sink.send(msg).await.is_err() {
                                                break;
                                            }
                                        }
                                        Err(e) => {
                                            tracing::error!("Failed to serialize message: {:?}", e);
                                            break;
                                        }
                                    }
                                }
                                Err(RecvError::Closed) => {
                                    break;
                                }
                                Err(RecvError::Lagged(_)) => {
                                }
                            }
                        }
                        _ = sink.closed() => {
                            break;
                        }
                    }
                }

                tracing::trace!("Subscription task ended (id: {:?})", sink.subscription_id());
            });

            Ok(())
        },
    )?;

    Ok(())
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
    #[ignore = "https://github.com/ChainSafe/forest/issues/4032"]
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
            }
        }
    }
}
