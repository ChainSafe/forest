// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod auth_layer;
mod channel;
mod client;
mod compression_layer;
mod error;
mod filter_layer;
mod filter_list;
pub mod json_validator;
mod log_layer;
mod metrics_layer;
mod parallel_batch_layer;
mod reflect;
mod registry;
mod request;
mod segregation_layer;
mod set_extension_layer;
pub mod types;
mod validation_layer;

use crate::db::DbImpl;
use crate::prelude::*;
use crate::rpc::eth::types::RandomHexStringIdProvider;
use crate::rpc::methods::eth::pubsub_trait::EthPubSubApiServer;
use crate::shim::clock::ChainEpoch;
use ahash::HashMap;
use clap::ValueEnum as _;
pub use client::Client;
pub use error::ServerError;
use eth::filter::EthEventHandler;
use filter_layer::FilterLayer;
pub use filter_list::FilterList;
use futures::future::Either;
use jsonrpsee::server::ServerConfig;
use log_layer::LogLayer;
pub use metrics_layer::MetricsMode;
use parallel_batch_layer::ParallelBatchLayer;
use reflect::Ctx;
pub use reflect::{ApiPaths, Permission, RpcMethod, RpcMethodExt};
pub use request::Request;
use schemars::Schema;
use segregation_layer::SegregationLayer;
use set_extension_layer::SetExtensionLayer;

pub use methods::*;

/// Protocol or transport-specific error
pub use jsonrpsee::core::ClientError;

/// Sentinel value, indicating no limit on how far back to search in the chain (all the way to genesis epoch).
pub const LOOKBACK_NO_LIMIT: ChainEpoch = -1;

/// The macro `callback` will be passed in each type that implements
/// [`RpcMethod`].
///
/// This is a macro because there is no way to abstract the `ARITY` on that
/// trait.
///
/// All methods should be entered here.
#[macro_export]
macro_rules! for_each_rpc_method {
    ($callback:path) => {
        // auth vertical
        $callback!($crate::rpc::auth::AuthNew);
        $callback!($crate::rpc::auth::AuthVerify);

        // beacon vertical
        $callback!($crate::rpc::beacon::BeaconGetEntry);

        // chain vertical
        $callback!($crate::rpc::chain::ChainPruneSnapshot);
        $callback!($crate::rpc::chain::ChainExport);
        $callback!($crate::rpc::chain::ChainGetBlock);
        $callback!($crate::rpc::chain::ChainGetBlockMessages);
        $callback!($crate::rpc::chain::ChainGetEvents);
        $callback!($crate::rpc::chain::ChainGetGenesis);
        $callback!($crate::rpc::chain::ChainGetFinalizedTipset);
        $callback!($crate::rpc::chain::ChainGetMessage);
        $callback!($crate::rpc::chain::ChainGetMessagesInTipset);
        $callback!($crate::rpc::chain::ChainGetMinBaseFee);
        $callback!($crate::rpc::chain::ChainGetParentMessages);
        $callback!($crate::rpc::chain::ChainGetParentReceipts);
        $callback!($crate::rpc::chain::ChainGetPath);
        $callback!($crate::rpc::chain::ChainGetTipSet);
        $callback!($crate::rpc::chain::ChainGetTipSetV2);
        $callback!($crate::rpc::chain::ChainGetTipSetFinalityStatus);
        $callback!($crate::rpc::chain::ChainGetTipSetAfterHeight);
        $callback!($crate::rpc::chain::ChainGetTipSetByHeight);
        $callback!($crate::rpc::chain::ChainHasObj);
        $callback!($crate::rpc::chain::ChainHead);
        $callback!($crate::rpc::chain::ChainReadObj);
        $callback!($crate::rpc::chain::ChainSetHead);
        $callback!($crate::rpc::chain::ChainStatObj);
        $callback!($crate::rpc::chain::ChainTipSetWeight);
        $callback!($crate::rpc::chain::ForestChainExport);
        $callback!($crate::rpc::chain::ForestChainExportDiff);
        $callback!($crate::rpc::chain::ForestChainExportStatus);
        $callback!($crate::rpc::chain::ForestChainExportCancel);
        $callback!($crate::rpc::chain::ForestChainExportReceiptsEvents);
        $callback!($crate::rpc::chain::ChainGetTipsetByParentState);

        // common vertical
        $callback!($crate::rpc::common::Session);
        $callback!($crate::rpc::common::Shutdown);
        $callback!($crate::rpc::common::StartTime);
        $callback!($crate::rpc::common::Version);

        // eth vertical
        $callback!($crate::rpc::eth::EthAccounts);
        $callback!($crate::rpc::eth::EthAddressToFilecoinAddress);
        $callback!($crate::rpc::eth::FilecoinAddressToEthAddress);
        $callback!($crate::rpc::eth::EthBaseFee);
        $callback!($crate::rpc::eth::BaseFeeByHeight);
        $callback!($crate::rpc::eth::EthBlockNumber);
        $callback!($crate::rpc::eth::EthCall);
        $callback!($crate::rpc::eth::EthChainId);
        $callback!($crate::rpc::eth::EthEstimateGas);
        $callback!($crate::rpc::eth::EthFeeHistory);
        $callback!($crate::rpc::eth::EthGasPrice);
        $callback!($crate::rpc::eth::EthGetBalance);
        $callback!($crate::rpc::eth::EthGetBlockByHash);
        $callback!($crate::rpc::eth::EthGetBlockByNumber);
        $callback!($crate::rpc::eth::EthGetBlockReceipts);
        $callback!($crate::rpc::eth::EthGetBlockReceiptsLimited);
        $callback!($crate::rpc::eth::EthGetBlockTransactionCountByHash);
        $callback!($crate::rpc::eth::EthGetBlockTransactionCountByNumber);
        $callback!($crate::rpc::eth::EthGetCode);
        $callback!($crate::rpc::eth::EthGetLogs);
        $callback!($crate::rpc::eth::EthGetFilterLogs);
        $callback!($crate::rpc::eth::EthGetFilterChanges);
        $callback!($crate::rpc::eth::EthGetMessageCidByTransactionHash);
        $callback!($crate::rpc::eth::EthGetStorageAt);
        $callback!($crate::rpc::eth::EthGetTransactionByHash);
        $callback!($crate::rpc::eth::EthGetTransactionByHashLimited);
        $callback!($crate::rpc::eth::EthGetTransactionCount);
        $callback!($crate::rpc::eth::EthGetTransactionHashByCid);
        $callback!($crate::rpc::eth::EthGetTransactionByBlockNumberAndIndex);
        $callback!($crate::rpc::eth::EthGetTransactionByBlockHashAndIndex);
        $callback!($crate::rpc::eth::EthMaxPriorityFeePerGas);
        $callback!($crate::rpc::eth::EthProtocolVersion);
        $callback!($crate::rpc::eth::EthGetTransactionReceipt);
        $callback!($crate::rpc::eth::EthGetTransactionReceiptLimited);
        $callback!($crate::rpc::eth::EthNewFilter);
        $callback!($crate::rpc::eth::EthNewPendingTransactionFilter);
        $callback!($crate::rpc::eth::EthNewBlockFilter);
        $callback!($crate::rpc::eth::EthUninstallFilter);
        $callback!($crate::rpc::eth::EthUnsubscribe);
        $callback!($crate::rpc::eth::EthSubscribe);
        $callback!($crate::rpc::eth::EthSyncing);
        $callback!($crate::rpc::eth::EthTraceBlock);
        $callback!($crate::rpc::eth::EthTraceCall);
        $callback!($crate::rpc::eth::EthTraceFilter);
        $callback!($crate::rpc::eth::EthTraceTransaction);
        $callback!($crate::rpc::eth::EthDebugTraceTransaction);
        $callback!($crate::rpc::eth::EthTraceReplayBlockTransactions);
        $callback!($crate::rpc::eth::Web3ClientVersion);
        $callback!($crate::rpc::eth::EthSendRawTransaction);
        $callback!($crate::rpc::eth::EthSendRawTransactionUntrusted);

        // gas vertical
        $callback!($crate::rpc::gas::GasEstimateFeeCap);
        $callback!($crate::rpc::gas::GasEstimateGasLimit);
        $callback!($crate::rpc::gas::GasEstimateGasPremium);
        $callback!($crate::rpc::gas::GasEstimateMessageGas);

        // market vertical
        $callback!($crate::rpc::market::MarketAddBalance);

        // miner vertical
        $callback!($crate::rpc::miner::MinerCreateBlock);
        $callback!($crate::rpc::miner::MinerGetBaseInfo);

        // mpool vertical
        $callback!($crate::rpc::mpool::MpoolBatchPush);
        $callback!($crate::rpc::mpool::MpoolBatchPushUntrusted);
        $callback!($crate::rpc::mpool::MpoolGetConfig);
        $callback!($crate::rpc::mpool::MpoolGetNonce);
        $callback!($crate::rpc::mpool::MpoolPending);
        $callback!($crate::rpc::mpool::MpoolPush);
        $callback!($crate::rpc::mpool::MpoolPushMessage);
        $callback!($crate::rpc::mpool::MpoolPushUntrusted);
        $callback!($crate::rpc::mpool::MpoolSelect);

        // msig vertical
        $callback!($crate::rpc::msig::MsigGetAvailableBalance);
        $callback!($crate::rpc::msig::MsigGetPending);
        $callback!($crate::rpc::msig::MsigGetVested);
        $callback!($crate::rpc::msig::MsigGetVestingSchedule);

        // net vertical
        $callback!($crate::rpc::net::NetAddrsListen);
        $callback!($crate::rpc::net::NetAgentVersion);
        $callback!($crate::rpc::net::NetAutoNatStatus);
        $callback!($crate::rpc::net::NetConnect);
        $callback!($crate::rpc::net::NetDisconnect);
        $callback!($crate::rpc::net::NetFindPeer);
        $callback!($crate::rpc::net::NetInfo);
        $callback!($crate::rpc::net::NetListening);
        $callback!($crate::rpc::net::NetPeers);
        $callback!($crate::rpc::net::NetProtectAdd);
        $callback!($crate::rpc::net::NetProtectList);
        $callback!($crate::rpc::net::NetProtectRemove);
        $callback!($crate::rpc::net::NetVersion);
        $callback!($crate::rpc::net::NetChainExchange);

        // node vertical
        $callback!($crate::rpc::node::NodeStatus);

        // state vertical
        $callback!($crate::rpc::state::StateAccountKey);
        $callback!($crate::rpc::state::StateCall);
        $callback!($crate::rpc::state::StateCirculatingSupply);
        $callback!($crate::rpc::state::ForestStateCompute);
        $callback!($crate::rpc::state::StateCompute);
        $callback!($crate::rpc::state::StateDealProviderCollateralBounds);
        $callback!($crate::rpc::state::StateFetchRoot);
        $callback!($crate::rpc::state::StateGetActor);
        $callback!($crate::rpc::state::StateGetActorV2);
        $callback!($crate::rpc::state::StateGetID);
        $callback!($crate::rpc::state::StateGetAllAllocations);
        $callback!($crate::rpc::state::StateGetAllClaims);
        $callback!($crate::rpc::state::StateGetAllocation);
        $callback!($crate::rpc::state::StateGetAllocationForPendingDeal);
        $callback!($crate::rpc::state::StateGetAllocationIdForPendingDeal);
        $callback!($crate::rpc::state::StateGetAllocations);
        $callback!($crate::rpc::state::StateGetBeaconEntry);
        $callback!($crate::rpc::state::StateGetClaim);
        $callback!($crate::rpc::state::StateGetClaims);
        $callback!($crate::rpc::state::StateGetNetworkParams);
        $callback!($crate::rpc::state::StateGetRandomnessDigestFromBeacon);
        $callback!($crate::rpc::state::StateGetRandomnessDigestFromTickets);
        $callback!($crate::rpc::state::StateGetRandomnessFromBeacon);
        $callback!($crate::rpc::state::StateGetRandomnessFromTickets);
        $callback!($crate::rpc::state::StateGetReceipt);
        $callback!($crate::rpc::state::StateListActors);
        $callback!($crate::rpc::state::StateListMessages);
        $callback!($crate::rpc::state::StateListMiners);
        $callback!($crate::rpc::state::StateLookupID);
        $callback!($crate::rpc::state::StateLookupRobustAddress);
        $callback!($crate::rpc::state::StateMarketBalance);
        $callback!($crate::rpc::state::StateMarketDeals);
        $callback!($crate::rpc::state::StateMarketParticipants);
        $callback!($crate::rpc::state::StateMarketStorageDeal);
        $callback!($crate::rpc::state::StateMinerActiveSectors);
        $callback!($crate::rpc::state::StateMinerAllocated);
        $callback!($crate::rpc::state::StateMinerAvailableBalance);
        $callback!($crate::rpc::state::StateMinerDeadlines);
        $callback!($crate::rpc::state::StateMinerFaults);
        $callback!($crate::rpc::state::StateMinerInfo);
        $callback!($crate::rpc::state::StateMinerInitialPledgeCollateral);
        $callback!($crate::rpc::state::StateMinerPartitions);
        $callback!($crate::rpc::state::StateMinerPower);
        $callback!($crate::rpc::state::StateMinerPreCommitDepositForPower);
        $callback!($crate::rpc::state::StateMinerProvingDeadline);
        $callback!($crate::rpc::state::StateMinerRecoveries);
        $callback!($crate::rpc::state::StateMinerSectorAllocated);
        $callback!($crate::rpc::state::StateMinerSectorCount);
        $callback!($crate::rpc::state::StateMinerSectors);
        $callback!($crate::rpc::state::StateNetworkName);
        $callback!($crate::rpc::state::StateNetworkVersion);
        $callback!($crate::rpc::state::StateActorInfo);
        $callback!($crate::rpc::state::StateReadState);
        $callback!($crate::rpc::state::StateDecodeParams);
        $callback!($crate::rpc::state::StateReplay);
        $callback!($crate::rpc::state::StateSearchMsg);
        $callback!($crate::rpc::state::StateSearchMsgLimited);
        $callback!($crate::rpc::state::StateSectorExpiration);
        $callback!($crate::rpc::state::StateSectorGetInfo);
        $callback!($crate::rpc::state::StateSectorPartition);
        $callback!($crate::rpc::state::StateSectorPreCommitInfo);
        $callback!($crate::rpc::state::StateSectorPreCommitInfoV0);
        $callback!($crate::rpc::state::StateVerifiedClientStatus);
        $callback!($crate::rpc::state::StateVerifiedRegistryRootKey);
        $callback!($crate::rpc::state::StateVerifierStatus);
        $callback!($crate::rpc::state::StateVMCirculatingSupplyInternal);
        $callback!($crate::rpc::state::StateWaitMsg);
        $callback!($crate::rpc::state::StateWaitMsgV0);
        $callback!($crate::rpc::state::StateMinerInitialPledgeForSector);

        // sync vertical
        $callback!($crate::rpc::sync::SyncCheckBad);
        $callback!($crate::rpc::sync::SyncMarkBad);
        $callback!($crate::rpc::sync::SyncSnapshotProgress);
        $callback!($crate::rpc::sync::SyncStatus);
        $callback!($crate::rpc::sync::SyncSubmitBlock);

        // wallet vertical
        $callback!($crate::rpc::wallet::WalletBalance);
        $callback!($crate::rpc::wallet::WalletDefaultAddress);
        $callback!($crate::rpc::wallet::WalletDelete);
        $callback!($crate::rpc::wallet::WalletExport);
        $callback!($crate::rpc::wallet::WalletHas);
        $callback!($crate::rpc::wallet::WalletImport);
        $callback!($crate::rpc::wallet::WalletList);
        $callback!($crate::rpc::wallet::WalletNew);
        $callback!($crate::rpc::wallet::WalletSetDefault);
        $callback!($crate::rpc::wallet::WalletSign);
        $callback!($crate::rpc::wallet::WalletSignMessage);
        $callback!($crate::rpc::wallet::WalletValidateAddress);
        $callback!($crate::rpc::wallet::WalletVerify);

        // f3
        $callback!($crate::rpc::f3::GetRawNetworkName);
        $callback!($crate::rpc::f3::F3GetCertificate);
        $callback!($crate::rpc::f3::F3GetECPowerTable);
        $callback!($crate::rpc::f3::F3GetF3PowerTable);
        $callback!($crate::rpc::f3::F3GetF3PowerTableByInstance);
        $callback!($crate::rpc::f3::F3IsRunning);
        $callback!($crate::rpc::f3::F3GetProgress);
        $callback!($crate::rpc::f3::F3GetManifest);
        $callback!($crate::rpc::f3::F3ListParticipants);
        $callback!($crate::rpc::f3::F3GetLatestCertificate);
        $callback!($crate::rpc::f3::F3GetOrRenewParticipationTicket);
        $callback!($crate::rpc::f3::F3Participate);
        $callback!($crate::rpc::f3::F3ExportLatestSnapshot);
        $callback!($crate::rpc::f3::GetHead);
        $callback!($crate::rpc::f3::GetParent);
        $callback!($crate::rpc::f3::GetParticipatingMinerIDs);
        $callback!($crate::rpc::f3::GetPowerTable);
        $callback!($crate::rpc::f3::GetTipset);
        $callback!($crate::rpc::f3::GetTipsetByEpoch);
        $callback!($crate::rpc::f3::Finalize);
        $callback!($crate::rpc::f3::ProtectPeer);
        $callback!($crate::rpc::f3::SignMessage);

        // misc
        $callback!($crate::rpc::misc::GetActorEventsRaw);
    };
}
use compression_layer::{COMPRESS_MIN_BODY_SIZE, CompressionLayer};
pub(crate) use for_each_rpc_method;
use sync::SnapshotProgressTracker;
use tower_http::sensitive_headers::SetSensitiveRequestHeadersLayer;

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

    for_each_rpc_method!(export);
}

/// Collects all the RPC method names and permission available in the Forest
pub fn collect_rpc_method_info() -> Vec<(&'static str, Permission)> {
    use crate::rpc::RpcMethod;

    let mut methods = Vec::new();

    macro_rules! add_method {
        ($ty:ty) => {
            methods.push((<$ty>::NAME, <$ty>::PERMISSION));
        };
    }

    for_each_rpc_method!(add_method);

    methods
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
    pub mod f3;
    pub mod gas;
    pub mod market;
    pub mod miner;
    pub mod misc;
    pub mod mpool;
    pub mod msig;
    pub mod net;
    pub mod node;
    pub mod state;
    pub mod sync;
    pub mod wallet;
}

use crate::rpc::auth_layer::{AuthLayer, resolve_claims};
pub use crate::rpc::channel::CANCEL_METHOD_NAME;
use crate::rpc::channel::RpcModule as FilRpcModule;
use crate::rpc::eth::pubsub::EthPubSub;
use crate::rpc::metrics_layer::MetricsLayer;
use crate::{chain_sync::network_context::SyncNetworkContext, key_management::KeyStore};

use crate::blocks::FullTipset;
use crate::utils::misc::env::env_or_default;
use jsonrpsee::{
    Methods,
    core::middleware::RpcServiceBuilder,
    server::{RpcModule, Server, StopHandle, TowerServiceBuilder},
};
use parking_lot::RwLock;
use std::env;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::sync::mpsc;
use tower::Service;

use crate::rpc::sync::SnapshotProgressState;
use openrpc_types::{self, ParamStructure};

pub const DEFAULT_PORT: u16 = 2345;

/// Request timeout read from environment variables
static DEFAULT_REQUEST_TIMEOUT: LazyLock<Duration> = LazyLock::new(|| {
    env::var("FOREST_RPC_DEFAULT_TIMEOUT")
        .ok()
        .and_then(|it| Duration::from_secs(it.parse().ok()?).into())
        .unwrap_or(Duration::from_secs(60))
});

/// Maximum concurrent connections accepted by the RPC server.
///
/// Configurable via `FOREST_RPC_MAX_CONNECTIONS`. The value also bounds the
/// TCP listen backlog so that bursts of connection attempts do not get
/// silently dropped by the kernel.
pub fn default_max_connections() -> u32 {
    static VALUE: LazyLock<u32> = LazyLock::new(|| {
        env::var("FOREST_RPC_MAX_CONNECTIONS")
            .ok()
            .and_then(|it| it.parse().ok())
            .unwrap_or(1000)
    });
    *VALUE
}

const MAX_REQUEST_BODY_SIZE: u32 = 64 * 1024 * 1024;

/// Maximum JSON-RPC response body size in bytes. Defaults to 64 MiB.
///
/// `eth_getTransactionReceipt` and `eth_getBlockReceipts` can return very
/// large responses for log-heavy transactions (a single tx emitting hundreds
/// of thousands of events can exceed 64 MiB). Operators serving such queries
/// can raise this with `FOREST_RPC_MAX_RESPONSE_BODY_SIZE` (in bytes).
static MAX_RESPONSE_BODY_SIZE: LazyLock<u32> =
    LazyLock::new(|| env_or_default("FOREST_RPC_MAX_RESPONSE_BODY_SIZE", MAX_REQUEST_BODY_SIZE));

/// This is where you store persistent data, or at least access to stateful
/// data.
pub struct RPCState {
    pub keystore: Arc<RwLock<KeyStore>>,
    pub state_manager: crate::state_manager::StateManager,
    pub mpool: crate::message_pool::MessagePool<crate::chain::ChainStore>,
    pub bad_blocks: Option<crate::chain_sync::BadBlockCache>,
    pub sync_status: crate::chain_sync::SyncStatus,
    pub eth_event_handler: Arc<EthEventHandler>,
    pub eth_logs_feed: std::sync::OnceLock<eth::pubsub::LogsFeed>,
    pub sync_network_context: SyncNetworkContext,
    pub tipset_send: flume::Sender<FullTipset>,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub snapshot_progress_tracker: SnapshotProgressTracker,
    pub shutdown: mpsc::Sender<()>,
    pub mpool_locker: crate::message_pool::MpoolLocker,
    pub nonce_tracker: crate::message_pool::NonceTracker,
    pub temp_dir: Arc<std::path::PathBuf>,
}

impl RPCState {
    pub fn beacon(&self) -> &Arc<crate::beacon::BeaconSchedule> {
        self.state_manager.beacon_schedule()
    }

    pub fn chain_store(&self) -> &crate::chain::ChainStore {
        self.state_manager.chain_store()
    }

    pub fn chain_index(&self) -> &crate::chain::index::ChainIndex {
        self.chain_store().chain_index()
    }

    pub fn chain_config(&self) -> &Arc<crate::networks::ChainConfig> {
        self.state_manager.chain_config()
    }

    pub fn db(&self) -> &DbImpl {
        self.state_manager.db()
    }

    pub fn db_owned(&self) -> DbImpl {
        self.state_manager.db_owned()
    }

    pub fn network_send(&self) -> &flume::Sender<crate::libp2p::NetworkMessage> {
        self.sync_network_context.network_send()
    }

    pub fn get_snapshot_progress_tracker(&self) -> SnapshotProgressState {
        self.snapshot_progress_tracker.state()
    }
}

#[derive(Clone)]
struct PerConnection<RpcMiddleware, HttpMiddleware> {
    stop_handle: StopHandle,
    svc_builder: Arc<TowerServiceBuilder<RpcMiddleware, HttpMiddleware>>,
    keystore: Arc<RwLock<KeyStore>>,
}

/// A bare HTTP response carrying just `status` and an empty body.
fn bare_http_response<B: Default>(status: http::StatusCode) -> http::Response<B> {
    http::Response::builder()
        .status(status)
        .body(B::default())
        .unwrap_or_else(|_| http::Response::new(B::default()))
}

pub async fn start_rpc(
    state: RPCState,
    rpc_listener: tokio::net::TcpListener,
    stop_handle: StopHandle,
    filter_list: Option<Arc<FilterList>>,
    metrics_mode: MetricsMode,
) -> anyhow::Result<()> {
    let filter_list = filter_list.unwrap_or_default();
    // `Arc` is needed because we will share the state between two modules
    let state = Arc::new(state);
    let keystore = state.keystore.shallow_clone();
    let mut modules = create_modules(state.shallow_clone());

    let mut pubsub_module = FilRpcModule::default();
    pubsub_module.register_channel("Filecoin.ChainNotify", {
        let state_clone = state.shallow_clone();
        move |params| chain::chain_notify(params, &state_clone)
    })?;

    for module in modules.values_mut() {
        // register eth subscription APIs
        module.merge(EthPubSub::new(state.shallow_clone()).into_rpc())?;
        module.merge(pubsub_module.clone())?;
    }

    let methods: Arc<HashMap<ApiPaths, Methods>> =
        Arc::new(modules.into_iter().map(|(k, v)| (k, v.into())).collect());

    let server_config = ServerConfig::builder()
        .max_request_body_size(MAX_REQUEST_BODY_SIZE)
        // Default size (10 MiB) is not enough for methods like `Filecoin.StateMinerActiveSectors`
        .max_response_body_size(*MAX_RESPONSE_BODY_SIZE)
        .max_connections(default_max_connections())
        .set_id_provider(RandomHexStringIdProvider::new())
        .build();
    let max_response_body_size = *MAX_RESPONSE_BODY_SIZE as usize;
    let per_conn = PerConnection {
        stop_handle: stop_handle.clone(),
        svc_builder: Server::builder()
            .set_config(server_config)
            .set_http_middleware(
                tower::ServiceBuilder::new()
                    .option_layer(COMPRESS_MIN_BODY_SIZE.map(CompressionLayer::new))
                    // Mark the `Authorization` request header as sensitive so it doesn't show in logs
                    .layer(SetSensitiveRequestHeadersLayer::new(std::iter::once(
                        http::header::AUTHORIZATION,
                    ))),
            )
            .to_service_builder()
            .into(),
        keystore,
    };
    tracing::info!("Ready for RPC connections");
    loop {
        let sock = tokio::select! {
        res = rpc_listener.accept() => {
            match res {
              Ok((stream, _remote_addr)) => {
                let _ = stream.set_nodelay(true); // Disable Nagle's algorithm
                stream
              }
              Err(e) => {
                tracing::error!("failed to accept v4 connection: {:?}", e);
                continue;
              }
            }
          }
          _ = per_conn.stop_handle.clone().shutdown() => break,
        };

        let svc = tower::service_fn({
            let methods = methods.shallow_clone();
            let per_conn = per_conn.clone();
            let filter_list = filter_list.shallow_clone();
            move |req: http::Request<_>| {
                let svc_or_result = if let Ok(path) = ApiPaths::from_uri(req.uri()) {
                    let methods = methods.get(&path).cloned().unwrap_or_default();
                    let PerConnection {
                        stop_handle,
                        svc_builder,
                        keystore,
                    } = per_conn.clone();
                    // Authenticate the connection once, here at the HTTP layer (for a
                    // WebSocket this is the upgrade request), before any JSON-RPC
                    // dispatch.
                    match resolve_claims(&keystore, req.headers().get(http::header::AUTHORIZATION))
                    {
                        Ok(claims) => {
                            // NOTE, the rpc middleware must be initialized here to be able to be created once per connection
                            // with data from the connection such as the headers in this example
                            let rpc_middleware = RpcServiceBuilder::new()
                                .layer(SetExtensionLayer { path })
                                .layer(SegregationLayer)
                                .layer(FilterLayer::new(filter_list.shallow_clone()))
                                .layer(validation_layer::JsonValidationLayer)
                                .layer(AuthLayer::new(claims))
                                .layer(LogLayer::default())
                                // `ParallelBatchLayer` fans a batch out into per-entry `call`s, so it must be
                                // outer to `MetricsLayer` for batched methods to be measured. Both must stay
                                // inner to the batch-transforming layers above.
                                .layer(ParallelBatchLayer::new(max_response_body_size))
                                .layer(MetricsLayer::new(metrics_mode));
                            Either::Left(
                                Arc::unwrap_or_clone(svc_builder)
                                    .set_rpc_middleware(rpc_middleware)
                                    .build(methods, stop_handle),
                            )
                        }
                        Err(reason) => {
                            tracing::debug!("rejecting unauthorized request: {reason}");
                            Either::Right(Ok(bare_http_response(http::StatusCode::UNAUTHORIZED)))
                        }
                    }
                } else {
                    Either::Right(Ok(bare_http_response(http::StatusCode::NOT_FOUND)))
                };
                async move {
                    match svc_or_result {
                        Either::Left(mut svc) => {
                            // https://github.com/rust-lang/rust/issues/102211 the error type can't be inferred
                            // to be `Box<dyn std::error::Error + Send + Sync>` so we need to convert it to a concrete type
                            // as workaround.
                            svc.call(req).await.map_err(|e| anyhow::anyhow!("{:?}", e))
                        }
                        Either::Right(result) => result,
                    }
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

fn create_modules(state: Arc<RPCState>) -> HashMap<ApiPaths, RpcModule<RPCState>> {
    let mut modules = HashMap::default();
    for api_version in ApiPaths::value_variants() {
        modules.insert(*api_version, RpcModule::from_arc(state.shallow_clone()));
    }
    macro_rules! register {
        ($ty:ty) => {
            // Register only non-subscription RPC methods.
            // Subscription methods are registered separately in the RPC module.
            if !<$ty>::SUBSCRIPTION {
                <$ty>::register(&mut modules, ParamStructure::ByPosition).unwrap();
            }
        };
    }
    for_each_rpc_method!(register);
    modules
}

/// If `include` is not [`None`], only methods that are listed will be returned
pub fn openrpc(path: ApiPaths, include: Option<&[&str]>) -> openrpc_types::OpenRPC {
    use schemars::generate::{SchemaGenerator, SchemaSettings};

    let mut methods = vec![];
    // spec says draft07
    let mut settings = SchemaSettings::draft07();
    // ..but uses `components`
    settings.definitions_path = "#/components/schemas/".into();
    let mut generator = SchemaGenerator::new(settings);
    macro_rules! callback {
        ($ty:ty) => {
            if <$ty>::API_PATHS.contains(path) {
                match include {
                    Some(include) => match include.contains(&<$ty>::NAME) {
                        true => {
                            methods.push(openrpc_types::ReferenceOr::Item(<$ty>::openrpc(
                                &mut generator,
                                ParamStructure::ByPosition,
                                &<$ty>::NAME,
                            )));
                            if let Some(alias) = &<$ty>::NAME_ALIAS {
                                methods.push(openrpc_types::ReferenceOr::Item(<$ty>::openrpc(
                                    &mut generator,
                                    ParamStructure::ByPosition,
                                    &alias,
                                )));
                            }
                        }
                        false => {}
                    },
                    None => {
                        methods.push(openrpc_types::ReferenceOr::Item(<$ty>::openrpc(
                            &mut generator,
                            ParamStructure::ByPosition,
                            &<$ty>::NAME,
                        )));
                        if let Some(alias) = &<$ty>::NAME_ALIAS {
                            methods.push(openrpc_types::ReferenceOr::Item(<$ty>::openrpc(
                                &mut generator,
                                ParamStructure::ByPosition,
                                &alias,
                            )));
                        }
                    }
                }
            }
        };
    }
    for_each_rpc_method!(callback);
    openrpc_types::OpenRPC {
        methods,
        components: Some(openrpc_types::Components {
            schemas: Some(
                generator
                    .take_definitions(false)
                    .into_iter()
                    .filter_map(|(k, v)| {
                        if let Ok(v) = Schema::try_from(v) {
                            Some((k, v))
                        } else {
                            None
                        }
                    })
                    .collect(),
            ),
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
    use super::*;
    use crate::{
        db::MemoryDB,
        networks::NetworkChain,
        rpc::{client::UrlClient, common::ShiftingVersion},
        tool::offline_server::server::offline_rpc_state,
    };
    use jsonrpsee::{
        core::{
            client::{BatchResponse, ClientT},
            params::BatchRequestBuilder,
        },
        server::stop_channel,
    };
    use std::net::{Ipv4Addr, SocketAddr};
    use tokio::task::JoinSet;

    // To update RPC specs:
    // `cargo test --lib -- rpc::tests::openrpc`
    // `cargo insta review`

    #[test]
    fn openrpc_v0() {
        openrpc(ApiPaths::V0);
    }

    #[test]
    fn openrpc_v1() {
        openrpc(ApiPaths::V1);
    }

    #[test]
    fn openrpc_v2() {
        openrpc(ApiPaths::V2);
    }

    fn openrpc(path: ApiPaths) {
        let spec = super::openrpc(path, None);
        insta::assert_yaml_snapshot!(path.path(), spec);
    }

    #[test]
    fn test_rpc_server() {
        const TIMEOUT: Duration = Duration::from_secs(5);
        let (done_tx, done_rx) = flume::bounded(1);
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async move { test_rpc_server_inner(done_tx).await });
        done_rx.recv().unwrap();
        // To mitigate the transient timeout issue
        rt.shutdown_timeout(TIMEOUT);
    }

    async fn test_rpc_server_inner(done_tx: flume::Sender<()>) {
        let chain = NetworkChain::Calibnet;
        let db = Arc::new(MemoryDB::default());
        let mut services = JoinSet::new();
        let (state, mut shutdown_recv) = offline_rpc_state(chain, db, None, None, &mut services)
            .await
            .unwrap();
        let block_delay_secs = state.chain_config().block_delay_secs;
        let shutdown_send = state.shutdown.clone();
        let jwt_read_permissions = vec!["read".to_owned()];
        let jwt_read = super::methods::auth::AuthNew::create_token(
            &state.keystore.read(),
            chrono::Duration::hours(1),
            jwt_read_permissions.clone(),
        )
        .unwrap();
        let rpc_listener =
            tokio::net::TcpListener::bind(SocketAddr::new(Ipv4Addr::LOCALHOST.into(), 0))
                .await
                .unwrap();
        let rpc_address = rpc_listener.local_addr().unwrap();
        let (stop_handle, server_handle) = stop_channel();

        // Start an RPC server

        let handle = tokio::spawn(start_rpc(
            state,
            rpc_listener,
            stop_handle,
            None,
            MetricsMode::Enabled,
        ));

        println!("sending a few http requests");

        let client = Client::from_url(
            format!("http://{}:{}/", rpc_address.ip(), rpc_address.port())
                .parse()
                .unwrap(),
        );

        let response = super::methods::common::Version::call(&client, ())
            .await
            .unwrap();
        assert_eq!(
            &response.version,
            &*crate::utils::version::FOREST_VERSION_STRING
        );
        assert_eq!(response.block_delay, block_delay_secs);
        assert_eq!(response.api_version, ShiftingVersion::new(2, 3, 0));

        let response = super::methods::auth::AuthVerify::call(&client, (jwt_read.clone(),))
            .await
            .unwrap();
        assert_eq!(response, jwt_read_permissions);

        // `AuthVerify` verifies a raw JWT; a `Bearer `-prefixed argument must fail.
        super::methods::auth::AuthVerify::call(&client, (format!("Bearer {jwt_read}"),))
            .await
            .unwrap_err();

        drop(client);

        // A bad token is rejected with a bare HTTP 401, before JSON-RPC dispatch.
        let http = reqwest::Client::new();
        let rpc_url = format!("http://{}:{}/rpc/v1", rpc_address.ip(), rpc_address.port());
        let jsonrpc_body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "Filecoin.Version",
            "params": [],
            "id": 0,
        });

        let bad_token = http
            .post(&rpc_url)
            .header(reqwest::header::AUTHORIZATION, "Bearer not-a-real-token")
            .json(&jsonrpc_body)
            .send()
            .await
            .unwrap();
        assert_eq!(bad_token.status(), reqwest::StatusCode::UNAUTHORIZED);

        // A valid token is not rejected at the HTTP layer.
        let good_token = http
            .post(&rpc_url)
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {jwt_read}"))
            .json(&jsonrpc_body)
            .send()
            .await
            .unwrap();
        assert_eq!(good_token.status(), reqwest::StatusCode::OK);

        println!("sending a few websocket requests");

        let client = Client::from_url(
            format!("ws://{}:{}/", rpc_address.ip(), rpc_address.port())
                .parse()
                .unwrap(),
        );

        let response = super::methods::auth::AuthVerify::call(&client, (jwt_read,))
            .await
            .unwrap();
        assert_eq!(response, jwt_read_permissions);

        drop(client);

        // Sending a batch request
        let client = UrlClient::new(
            format!("http://{}:{}/rpc/v1", rpc_address.ip(), rpc_address.port())
                .parse()
                .unwrap(),
            None,
        )
        .await
        .unwrap();
        let mut batch_request_builder = BatchRequestBuilder::new();
        let empty_payload: [(); 0] = [];
        batch_request_builder
            .insert("Filecoin.Version", empty_payload)
            .unwrap();
        batch_request_builder
            .insert("eth_chainId", empty_payload)
            .unwrap();
        let batch_response: BatchResponse<serde_json::Value> =
            client.batch_request(batch_request_builder).await.unwrap();
        assert_eq!(batch_response.len(), 2);
        assert_eq!(batch_response.num_successful_calls(), 2);
        assert_eq!(batch_response.num_failed_calls(), 0);

        // `eth_chainId` is only ever requested inside the batch above, so its presence in the RPC
        // timing metric proves batched methods flow through `MetricsLayer`. Guards against batch
        // entries bypassing metrics (which happens if `MetricsLayer` is outer to `ParallelBatchLayer`).
        let mut encoded = String::new();
        prometheus_client::encoding::text::encode_registry(
            &mut encoded,
            &crate::metrics::default_registry(),
        )
        .unwrap();
        let recorded = encoded.lines().any(|line| {
            line.starts_with("rpc_processing_time_count{")
                && line.contains(r#"method="eth_chainId""#)
                && line
                    .rsplit(' ')
                    .next()
                    .and_then(|v| v.parse::<u64>().ok())
                    .is_some_and(|count| count == 1)
        });
        assert!(
            recorded,
            "batched method `eth_chainId` was not recorded in rpc_processing_time:\n{encoded}"
        );

        // Gracefully shutdown the RPC server
        println!("sending shutdown signal");
        shutdown_send.send(()).await.unwrap();
        println!("waiting on shutdown receiver");
        shutdown_recv.recv().await;
        println!("sending server stop signal");
        server_handle.stop().unwrap();
        println!("waiting on graceful shutdown");
        handle.await.unwrap().unwrap();
        println!("done");
        done_tx.send(()).unwrap();
    }
}
