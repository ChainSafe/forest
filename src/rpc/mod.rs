// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod auth_layer;
mod channel;
mod client;
mod request;

pub use client::Client;
pub use error::ServerError;
use futures::FutureExt as _;
use reflect::Ctx;
pub use reflect::{ApiPath, ApiPaths, RpcMethod, RpcMethodExt};
pub use request::Request;
mod error;
mod reflect;
pub mod types;
pub use methods::*;
use reflect::Permission;

/// Protocol or transport-specific error
pub use jsonrpsee::core::ClientError;

/// The macro `callback` will be passed in each type that implements
/// [`RpcMethod`].
///
/// This is a macro because there is no way to abstract the `ARITY` on that
/// trait.
///
/// All methods should be entered here.
macro_rules! for_each_method {
    ($callback:path) => {
        // auth vertical
        $callback!(crate::rpc::auth::AuthNew);
        $callback!(crate::rpc::auth::AuthVerify);

        // beacon vertical
        $callback!(crate::rpc::beacon::BeaconGetEntry);

        // chain vertical
        $callback!(crate::rpc::chain::ChainGetMessage);
        $callback!(crate::rpc::chain::ChainGetParentMessages);
        $callback!(crate::rpc::chain::ChainGetParentReceipts);
        $callback!(crate::rpc::chain::ChainGetMessagesInTipset);
        $callback!(crate::rpc::chain::ChainExport);
        $callback!(crate::rpc::chain::ChainReadObj);
        $callback!(crate::rpc::chain::ChainHasObj);
        $callback!(crate::rpc::chain::ChainStatObj);
        $callback!(crate::rpc::chain::ChainGetBlockMessages);
        $callback!(crate::rpc::chain::ChainGetPath);
        $callback!(crate::rpc::chain::ChainGetTipSetByHeight);
        $callback!(crate::rpc::chain::ChainGetTipSetAfterHeight);
        $callback!(crate::rpc::chain::ChainGetGenesis);
        $callback!(crate::rpc::chain::ChainHead);
        $callback!(crate::rpc::chain::ChainGetBlock);
        $callback!(crate::rpc::chain::ChainGetTipSet);
        $callback!(crate::rpc::chain::ChainSetHead);
        $callback!(crate::rpc::chain::ChainGetMinBaseFee);
        $callback!(crate::rpc::chain::ChainTipSetWeight);

        // common vertical
        $callback!(crate::rpc::common::Session);
        $callback!(crate::rpc::common::Version);
        $callback!(crate::rpc::common::Shutdown);
        $callback!(crate::rpc::common::StartTime);

        // eth vertical
        $callback!(crate::rpc::eth::Web3ClientVersion);
        $callback!(crate::rpc::eth::EthSyncing);
        $callback!(crate::rpc::eth::EthAccounts);
        $callback!(crate::rpc::eth::EthBlockNumber);
        $callback!(crate::rpc::eth::EthChainId);
        $callback!(crate::rpc::eth::EthEstimateGas);
        $callback!(crate::rpc::eth::EthFeeHistory);
        $callback!(crate::rpc::eth::EthGetCode);
        $callback!(crate::rpc::eth::EthGetStorageAt);
        $callback!(crate::rpc::eth::EthGasPrice);
        $callback!(crate::rpc::eth::EthGetBalance);
        $callback!(crate::rpc::eth::EthGetBlockByHash);
        $callback!(crate::rpc::eth::EthGetBlockByNumber);
        $callback!(crate::rpc::eth::EthGetBlockTransactionCountByHash);
        $callback!(crate::rpc::eth::EthGetBlockTransactionCountByNumber);
        $callback!(crate::rpc::eth::EthGetMessageCidByTransactionHash);
        $callback!(crate::rpc::eth::EthGetTransactionCount);
        $callback!(crate::rpc::eth::EthMaxPriorityFeePerGas);
        $callback!(crate::rpc::eth::EthProtocolVersion);
        $callback!(crate::rpc::eth::EthGetTransactionHashByCid);

        // gas vertical
        $callback!(crate::rpc::gas::GasEstimateGasLimit);
        $callback!(crate::rpc::gas::GasEstimateMessageGas);
        $callback!(crate::rpc::gas::GasEstimateFeeCap);
        $callback!(crate::rpc::gas::GasEstimateGasPremium);

        // miner vertical
        $callback!(crate::rpc::miner::MinerCreateBlock);
        $callback!(crate::rpc::miner::MinerGetBaseInfo);

        // mpool vertical
        $callback!(crate::rpc::mpool::MpoolGetNonce);
        $callback!(crate::rpc::mpool::MpoolPending);
        $callback!(crate::rpc::mpool::MpoolSelect);
        $callback!(crate::rpc::mpool::MpoolPush);
        $callback!(crate::rpc::mpool::MpoolPushUntrusted);
        $callback!(crate::rpc::mpool::MpoolPushMessage);

        // msig vertical
        $callback!(crate::rpc::msig::MsigGetAvailableBalance);
        $callback!(crate::rpc::msig::MsigGetPending);
        $callback!(crate::rpc::msig::MsigGetVested);
        $callback!(crate::rpc::msig::MsigGetVestingSchedule);

        // net vertical
        $callback!(crate::rpc::net::NetAddrsListen);
        $callback!(crate::rpc::net::NetPeers);
        $callback!(crate::rpc::net::NetListening);
        $callback!(crate::rpc::net::NetInfo);
        $callback!(crate::rpc::net::NetConnect);
        $callback!(crate::rpc::net::NetDisconnect);
        $callback!(crate::rpc::net::NetAgentVersion);
        $callback!(crate::rpc::net::NetAutoNatStatus);
        $callback!(crate::rpc::net::NetVersion);
        $callback!(crate::rpc::net::NetProtectAdd);
        $callback!(crate::rpc::net::NetFindPeer);

        // node vertical
        $callback!(crate::rpc::node::NodeStatus);

        // state vertical
        $callback!(crate::rpc::state::StateCall);
        $callback!(crate::rpc::state::StateGetBeaconEntry);
        $callback!(crate::rpc::state::StateListMessages);
        $callback!(crate::rpc::state::StateGetNetworkParams);
        $callback!(crate::rpc::state::StateNetworkName);
        $callback!(crate::rpc::state::StateReplay);
        $callback!(crate::rpc::state::StateSectorGetInfo);
        $callback!(crate::rpc::state::StateSectorPreCommitInfoV0);
        $callback!(crate::rpc::state::StateSectorPreCommitInfo);
        $callback!(crate::rpc::state::StateAccountKey);
        $callback!(crate::rpc::state::StateLookupID);
        $callback!(crate::rpc::state::StateGetActor);
        $callback!(crate::rpc::state::StateMinerInfo);
        $callback!(crate::rpc::state::StateMinerActiveSectors);
        $callback!(crate::rpc::state::StateMinerAllocated);
        $callback!(crate::rpc::state::StateMinerPartitions);
        $callback!(crate::rpc::state::StateMinerSectors);
        $callback!(crate::rpc::state::StateMinerSectorCount);
        $callback!(crate::rpc::state::StateMinerSectorAllocated);
        $callback!(crate::rpc::state::StateMinerPower);
        $callback!(crate::rpc::state::StateMinerDeadlines);
        $callback!(crate::rpc::state::StateMinerProvingDeadline);
        $callback!(crate::rpc::state::StateMinerFaults);
        $callback!(crate::rpc::state::StateMinerRecoveries);
        $callback!(crate::rpc::state::StateMinerAvailableBalance);
        $callback!(crate::rpc::state::StateMinerInitialPledgeCollateral);
        $callback!(crate::rpc::state::StateGetReceipt);
        $callback!(crate::rpc::state::StateGetRandomnessFromTickets);
        $callback!(crate::rpc::state::StateGetRandomnessDigestFromTickets);
        $callback!(crate::rpc::state::StateGetRandomnessFromBeacon);
        $callback!(crate::rpc::state::StateGetRandomnessDigestFromBeacon);
        $callback!(crate::rpc::state::StateReadState);
        $callback!(crate::rpc::state::StateCirculatingSupply);
        $callback!(crate::rpc::state::StateVerifiedClientStatus);
        $callback!(crate::rpc::state::StateVMCirculatingSupplyInternal);
        $callback!(crate::rpc::state::StateListMiners);
        $callback!(crate::rpc::state::StateListActors);
        $callback!(crate::rpc::state::StateNetworkVersion);
        $callback!(crate::rpc::state::StateMarketBalance);
        $callback!(crate::rpc::state::StateMarketParticipants);
        $callback!(crate::rpc::state::StateMarketDeals);
        $callback!(crate::rpc::state::StateDealProviderCollateralBounds);
        $callback!(crate::rpc::state::StateMarketStorageDeal);
        $callback!(crate::rpc::state::StateWaitMsgV0);
        $callback!(crate::rpc::state::StateWaitMsg);
        $callback!(crate::rpc::state::StateSearchMsg);
        $callback!(crate::rpc::state::StateSearchMsgLimited);
        $callback!(crate::rpc::state::StateFetchRoot);
        $callback!(crate::rpc::state::StateCompute);
        $callback!(crate::rpc::state::StateMinerPreCommitDepositForPower);
        $callback!(crate::rpc::state::StateVerifiedRegistryRootKey);
        $callback!(crate::rpc::state::StateVerifierStatus);
        $callback!(crate::rpc::state::StateGetClaim);
        $callback!(crate::rpc::state::StateGetClaims);
        $callback!(crate::rpc::state::StateGetAllClaims);
        $callback!(crate::rpc::state::StateGetAllocation);
        $callback!(crate::rpc::state::StateGetAllocations);
        $callback!(crate::rpc::state::StateGetAllAllocations);
        $callback!(crate::rpc::state::StateGetAllocationIdForPendingDeal);
        $callback!(crate::rpc::state::StateGetAllocationForPendingDeal);
        $callback!(crate::rpc::state::StateSectorExpiration);
        $callback!(crate::rpc::state::StateSectorPartition);
        $callback!(crate::rpc::state::StateLookupRobustAddress);

        // sync vertical
        $callback!(crate::rpc::sync::SyncCheckBad);
        $callback!(crate::rpc::sync::SyncMarkBad);
        $callback!(crate::rpc::sync::SyncState);
        $callback!(crate::rpc::sync::SyncSubmitBlock);

        // wallet vertical
        $callback!(crate::rpc::wallet::WalletBalance);
        $callback!(crate::rpc::wallet::WalletDefaultAddress);
        $callback!(crate::rpc::wallet::WalletExport);
        $callback!(crate::rpc::wallet::WalletHas);
        $callback!(crate::rpc::wallet::WalletImport);
        $callback!(crate::rpc::wallet::WalletList);
        $callback!(crate::rpc::wallet::WalletNew);
        $callback!(crate::rpc::wallet::WalletSetDefault);
        $callback!(crate::rpc::wallet::WalletSign);
        $callback!(crate::rpc::wallet::WalletSignMessage);
        $callback!(crate::rpc::wallet::WalletValidateAddress);
        $callback!(crate::rpc::wallet::WalletVerify);
        $callback!(crate::rpc::wallet::WalletDelete);
    };
}
pub(crate) use for_each_method;

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

    for_each_method!(export);
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
    pub mod miner;
    pub mod mpool;
    pub mod msig;
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

use openrpc_types::{self, ParamStructure};

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
    pub state_manager: Arc<crate::state_manager::StateManager<DB>>,
    pub mpool: Arc<crate::message_pool::MessagePool<crate::message_pool::MpoolRpcProvider<DB>>>,
    pub bad_blocks: Arc<crate::chain_sync::BadBlockCache>,
    pub sync_state: Arc<parking_lot::RwLock<crate::chain_sync::SyncState>>,
    pub network_send: flume::Sender<crate::libp2p::NetworkMessage>,
    pub network_name: String,
    pub tipset_send: flume::Sender<Arc<Tipset>>,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub shutdown: mpsc::Sender<()>,
}

impl<DB: Blockstore> RPCState<DB> {
    pub fn beacon(&self) -> &Arc<crate::beacon::BeaconSchedule> {
        self.state_manager.beacon_schedule()
    }

    pub fn chain_store(&self) -> &Arc<crate::chain::ChainStore<DB>> {
        self.state_manager.chain_store()
    }

    pub fn chain_index(&self) -> &Arc<crate::chain::index::ChainIndex<Arc<DB>>> {
        &self.chain_store().chain_index
    }

    pub fn chain_config(&self) -> &Arc<crate::networks::ChainConfig> {
        self.state_manager.chain_config()
    }

    pub fn store(&self) -> &DB {
        self.chain_store().blockstore()
    }

    pub fn store_owned(&self) -> Arc<DB> {
        self.state_manager.blockstore_owned()
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
    let mut module = create_module(state.clone());

    let mut pubsub_module = FilRpcModule::default();

    pubsub_module.register_channel("Filecoin.ChainNotify", {
        let state_clone = state.clone();
        move |params| chain::chain_notify(params, &state_clone)
    })?;
    module.merge(pubsub_module)?;

    let (stop_handle, _server_handle) = stop_channel();

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

    let listener = tokio::net::TcpListener::bind(rpc_endpoint).await.unwrap();
    tracing::info!("Ready for RPC connections");
    loop {
        let sock = tokio::select! {
        res = listener.accept() => {
            match res {
              Ok((stream, _remote_addr)) => stream,
              Err(e) => {
                tracing::error!("failed to accept v4 connection: {:?}", e);
                continue;
              }
            }
          }
          _ = per_conn.stop_handle.clone().shutdown() => break,
        };

        let svc = tower::service_fn({
            let per_conn = per_conn.clone();
            move |req| {
                let is_websocket = jsonrpsee::server::ws::is_upgrade_request(&req);
                let PerConnection {
                    methods,
                    stop_handle,
                    svc_builder,
                    keystore,
                } = per_conn.clone();
                // NOTE, the rpc middleware must be initialized here to be able to created once per connection
                // with data from the connection such as the headers in this example
                let headers = req.headers().clone();
                let rpc_middleware = RpcServiceBuilder::new().layer(AuthLayer {
                    headers,
                    keystore: keystore.clone(),
                });
                let mut jsonrpsee_svc = svc_builder
                    .set_rpc_middleware(rpc_middleware)
                    .build(methods, stop_handle);

                if is_websocket {
                    // Utilize the session close future to know when the actual WebSocket
                    // session was closed.
                    let session_close = jsonrpsee_svc.on_session_closed();

                    // A little bit weird API but the response to HTTP request must be returned below
                    // and we spawn a task to register when the session is closed.
                    tokio::spawn(async move {
                        session_close.await;
                        tracing::trace!("Closed WebSocket connection");
                    });

                    async move {
                        tracing::trace!("Opened WebSocket connection");
                        // https://github.com/rust-lang/rust/issues/102211 the error type can't be inferred
                        // to be `Box<dyn std::error::Error + Send + Sync>` so we need to convert it to a concrete type
                        // as workaround.
                        jsonrpsee_svc
                            .call(req)
                            .await
                            .map_err(|e| anyhow::anyhow!("{:?}", e))
                    }
                    .boxed()
                } else {
                    // HTTP.
                    async move {
                        tracing::trace!("Opened HTTP connection");
                        let rp = jsonrpsee_svc.call(req).await;
                        tracing::trace!("Closed HTTP connection");
                        // https://github.com/rust-lang/rust/issues/102211 the error type can't be inferred
                        // to be `Box<dyn std::error::Error + Send + Sync>` so we need to convert it to a concrete type
                        // as workaround.
                        rp.map_err(|e| anyhow::anyhow!("{:?}", e))
                    }
                    .boxed()
                }
            }
        });

        tokio::spawn(jsonrpsee::server::serve_with_graceful_shutdown(
            sock,
            svc,
            stop_handle.clone().shutdown(),
        ));
    }

    Ok(())
}

fn create_module<DB>(state: Arc<RPCState<DB>>) -> RpcModule<RPCState<DB>>
where
    DB: Blockstore + Send + Sync + 'static,
{
    let mut module = RpcModule::from_arc(state);
    macro_rules! register {
        ($ty:ty) => {
            <$ty>::register(&mut module, ParamStructure::ByPosition).unwrap();
            // Optionally register an alias for the method.
            <$ty>::register_alias(&mut module).unwrap();
        };
    }
    for_each_method!(register);
    module
}

/// If `include` is not [`None`], only methods that are listed will be returned
pub fn openrpc(path: ApiPath, include: Option<&[&str]>) -> openrpc_types::OpenRPC {
    use schemars::gen::{SchemaGenerator, SchemaSettings};
    let mut methods = vec![];
    // spec says draft07
    let mut settings = SchemaSettings::draft07();
    // ..but uses `components`
    settings.definitions_path = String::from("#/components/schemas/");
    let mut gen = SchemaGenerator::new(settings);
    macro_rules! callback {
        ($ty:ty) => {
            if <$ty>::API_PATHS.contains(path) {
                match include {
                    Some(include) => match include.contains(&<$ty>::NAME) {
                        true => methods.push(openrpc_types::ReferenceOr::Item(<$ty>::openrpc(
                            &mut gen,
                            ParamStructure::ByPosition,
                        ))),
                        false => {}
                    },
                    None => methods.push(openrpc_types::ReferenceOr::Item(<$ty>::openrpc(
                        &mut gen,
                        ParamStructure::ByPosition,
                    ))),
                }
            }
        };
    }
    for_each_method!(callback);
    openrpc_types::OpenRPC {
        methods,
        components: Some(openrpc_types::Components {
            schemas: Some(gen.take_definitions().into_iter().collect()),
            ..Default::default()
        }),
        openrpc: openrpc_types::OPEN_RPC_SPECIFICATION_VERSION,
        info: openrpc_types::Info {
            title: String::from("forest"),
            version: env!("CARGO_PKG_VERSION").into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use crate::rpc::ApiPath;

    // `cargo test --lib -- --exact 'rpc::tests::openrpc'`
    // `cargo insta review`
    #[test]
    fn openrpc() {
        for path in [ApiPath::V0, ApiPath::V1] {
            let _spec = super::openrpc(path, None);
            // TODO(aatifsyed): https://github.com/ChainSafe/forest/issues/4032
            //                  this is disabled because it causes lots of merge
            //                  conflicts.
            //                  We should consider re-enabling it when our RPC is
            //                  more stable.
            //                  (We still run this test to make sure we're not
            //                  violating other invariants)
            #[cfg(never)]
            insta::assert_yaml_snapshot!(_spec);
        }
    }
}
