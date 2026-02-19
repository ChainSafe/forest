// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::rpc::methods::eth::pubsub_trait::EthPubSubApiServer;
mod auth_layer;
mod channel;
mod client;
mod filter_layer;
mod filter_list;
pub mod json_validator;
mod log_layer;
mod metrics_layer;
mod request;
mod segregation_layer;
mod set_extension_layer;
mod validation_layer;

use crate::rpc::eth::types::RandomHexStringIdProvider;
use crate::shim::clock::ChainEpoch;
use clap::ValueEnum as _;
pub use client::Client;
pub use error::ServerError;
use eth::filter::EthEventHandler;
use filter_layer::FilterLayer;
pub use filter_list::FilterList;
use futures::FutureExt as _;
use jsonrpsee::server::ServerConfig;
use log_layer::LogLayer;
use reflect::Ctx;
pub use reflect::{ApiPaths, Permission, RpcMethod, RpcMethodExt};
pub use request::Request;
use schemars::Schema;
use segregation_layer::SegregationLayer;
use set_extension_layer::SetExtensionLayer;
mod error;
mod reflect;
use ahash::HashMap;
mod registry;
pub mod types;

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
        $callback!($crate::rpc::eth::EthBlockNumber);
        $callback!($crate::rpc::eth::EthCall);
        $callback!($crate::rpc::eth::EthCallV2);
        $callback!($crate::rpc::eth::EthChainId);
        $callback!($crate::rpc::eth::EthEstimateGas);
        $callback!($crate::rpc::eth::EthEstimateGasV2);
        $callback!($crate::rpc::eth::EthFeeHistory);
        $callback!($crate::rpc::eth::EthFeeHistoryV2);
        $callback!($crate::rpc::eth::EthGasPrice);
        $callback!($crate::rpc::eth::EthGetBalance);
        $callback!($crate::rpc::eth::EthGetBalanceV2);
        $callback!($crate::rpc::eth::EthGetBlockByHash);
        $callback!($crate::rpc::eth::EthGetBlockByNumber);
        $callback!($crate::rpc::eth::EthGetBlockByNumberV2);
        $callback!($crate::rpc::eth::EthGetBlockReceipts);
        $callback!($crate::rpc::eth::EthGetBlockReceiptsV2);
        $callback!($crate::rpc::eth::EthGetBlockReceiptsLimited);
        $callback!($crate::rpc::eth::EthGetBlockReceiptsLimitedV2);
        $callback!($crate::rpc::eth::EthGetBlockTransactionCountByHash);
        $callback!($crate::rpc::eth::EthGetBlockTransactionCountByNumber);
        $callback!($crate::rpc::eth::EthGetBlockTransactionCountByNumberV2);
        $callback!($crate::rpc::eth::EthGetCode);
        $callback!($crate::rpc::eth::EthGetCodeV2);
        $callback!($crate::rpc::eth::EthGetLogs);
        $callback!($crate::rpc::eth::EthGetFilterLogs);
        $callback!($crate::rpc::eth::EthGetFilterChanges);
        $callback!($crate::rpc::eth::EthGetMessageCidByTransactionHash);
        $callback!($crate::rpc::eth::EthGetStorageAt);
        $callback!($crate::rpc::eth::EthGetStorageAtV2);
        $callback!($crate::rpc::eth::EthGetTransactionByHash);
        $callback!($crate::rpc::eth::EthGetTransactionByHashLimited);
        $callback!($crate::rpc::eth::EthGetTransactionCount);
        $callback!($crate::rpc::eth::EthGetTransactionCountV2);
        $callback!($crate::rpc::eth::EthGetTransactionHashByCid);
        $callback!($crate::rpc::eth::EthGetTransactionByBlockNumberAndIndex);
        $callback!($crate::rpc::eth::EthGetTransactionByBlockNumberAndIndexV2);
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
        $callback!($crate::rpc::eth::EthTraceBlockV2);
        $callback!($crate::rpc::eth::EthTraceCall);
        $callback!($crate::rpc::eth::EthTraceFilter);
        $callback!($crate::rpc::eth::EthTraceTransaction);
        $callback!($crate::rpc::eth::EthTraceReplayBlockTransactions);
        $callback!($crate::rpc::eth::EthTraceReplayBlockTransactionsV2);
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
pub(crate) use for_each_rpc_method;
use sync::SnapshotProgressTracker;
use tower_http::compression::CompressionLayer;
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

use crate::rpc::auth_layer::AuthLayer;
pub use crate::rpc::channel::CANCEL_METHOD_NAME;
use crate::rpc::channel::RpcModule as FilRpcModule;
use crate::rpc::eth::pubsub::EthPubSub;
use crate::rpc::metrics_layer::MetricsLayer;
use crate::{chain_sync::network_context::SyncNetworkContext, key_management::KeyStore};

use crate::blocks::FullTipset;
use fvm_ipld_blockstore::Blockstore;
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

/// Default maximum connections for the RPC server. This needs to be high enough to
/// accommodate the regular usage for RPC providers.
static DEFAULT_MAX_CONNECTIONS: LazyLock<u32> = LazyLock::new(|| {
    env::var("FOREST_RPC_MAX_CONNECTIONS")
        .ok()
        .and_then(|it| it.parse().ok())
        .unwrap_or(1000)
});

const MAX_REQUEST_BODY_SIZE: u32 = 64 * 1024 * 1024;
const MAX_RESPONSE_BODY_SIZE: u32 = MAX_REQUEST_BODY_SIZE;

/// This is where you store persistent data, or at least access to stateful
/// data.
pub struct RPCState<DB> {
    pub keystore: Arc<RwLock<KeyStore>>,
    pub state_manager: Arc<crate::state_manager::StateManager<DB>>,
    pub mpool: Arc<crate::message_pool::MessagePool<crate::message_pool::MpoolRpcProvider<DB>>>,
    pub bad_blocks: Option<Arc<crate::chain_sync::BadBlockCache>>,
    pub msgs_in_tipset: Arc<crate::chain::store::MsgsInTipsetCache>,
    pub sync_status: crate::chain_sync::SyncStatus,
    pub eth_event_handler: Arc<EthEventHandler>,
    pub sync_network_context: SyncNetworkContext<DB>,
    pub tipset_send: flume::Sender<FullTipset>,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub snapshot_progress_tracker: SnapshotProgressTracker,
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
        self.chain_store().chain_index()
    }

    pub fn chain_config(&self) -> &Arc<crate::networks::ChainConfig> {
        self.state_manager.chain_config()
    }

    pub fn store(&self) -> &Arc<DB> {
        self.chain_store().blockstore()
    }

    pub fn store_owned(&self) -> Arc<DB> {
        self.state_manager.blockstore_owned()
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
    svc_builder: TowerServiceBuilder<RpcMiddleware, HttpMiddleware>,
    keystore: Arc<RwLock<KeyStore>>,
}

pub async fn start_rpc<DB>(
    state: RPCState<DB>,
    rpc_listener: tokio::net::TcpListener,
    stop_handle: StopHandle,
    filter_list: Option<FilterList>,
) -> anyhow::Result<()>
where
    DB: Blockstore + Send + Sync + 'static,
{
    let filter_list = filter_list.unwrap_or_default();
    // `Arc` is needed because we will share the state between two modules
    let state = Arc::new(state);
    let keystore = state.keystore.clone();
    let mut modules = create_modules(state.clone());

    let mut pubsub_module = FilRpcModule::default();
    pubsub_module.register_channel("Filecoin.ChainNotify", {
        let state_clone = state.clone();
        move |params| chain::chain_notify(params, &state_clone)
    })?;

    for module in modules.values_mut() {
        // register eth subscription APIs
        module.merge(EthPubSub::new(state.clone()).into_rpc())?;
        module.merge(pubsub_module.clone())?;
    }

    let methods: Arc<HashMap<ApiPaths, Methods>> =
        Arc::new(modules.into_iter().map(|(k, v)| (k, v.into())).collect());

    let per_conn = PerConnection {
        stop_handle: stop_handle.clone(),
        svc_builder: Server::builder()
            .set_config(
                ServerConfig::builder()
                    // Default size (10 MiB) is not enough for methods like `Filecoin.StateMinerActiveSectors`
                    .max_request_body_size(MAX_REQUEST_BODY_SIZE)
                    .max_response_body_size(MAX_RESPONSE_BODY_SIZE)
                    .max_connections(*DEFAULT_MAX_CONNECTIONS)
                    .set_id_provider(RandomHexStringIdProvider::new())
                    .build(),
            )
            .set_http_middleware(
                tower::ServiceBuilder::new()
                    .layer(CompressionLayer::new())
                    // Mark the `Authorization` request header as sensitive so it doesn't show in logs
                    .layer(SetSensitiveRequestHeadersLayer::new(std::iter::once(
                        http::header::AUTHORIZATION,
                    ))),
            )
            .to_service_builder(),
        keystore,
    };
    tracing::info!("Ready for RPC connections");
    loop {
        let sock = tokio::select! {
        res = rpc_listener.accept() => {
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
            let methods = methods.clone();
            let per_conn = per_conn.clone();
            let filter_list = filter_list.clone();
            move |req| {
                let is_websocket = jsonrpsee::server::ws::is_upgrade_request(&req);
                let path = if let Ok(p) = ApiPaths::from_uri(req.uri()) {
                    p
                } else {
                    return async move {
                        Ok(http::Response::builder()
                            .status(http::StatusCode::NOT_FOUND)
                            .body(Default::default())
                            .unwrap_or_else(|_| http::Response::new(Default::default())))
                    }
                    .boxed();
                };
                let methods = methods.get(&path).cloned().unwrap_or_default();
                let PerConnection {
                    stop_handle,
                    svc_builder,
                    keystore,
                } = per_conn.clone();
                // NOTE, the rpc middleware must be initialized here to be able to be created once per connection
                // with data from the connection such as the headers in this example
                let headers = req.headers().clone();
                let rpc_middleware = RpcServiceBuilder::new()
                    .layer(SetExtensionLayer { path })
                    .layer(SegregationLayer)
                    .layer(FilterLayer::new(filter_list.clone()))
                    .layer(validation_layer::JsonValidationLayer)
                    .layer(AuthLayer {
                        headers,
                        keystore: keystore.clone(),
                    })
                    .layer(LogLayer::default())
                    .layer(MetricsLayer::default());
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

fn create_modules<DB>(state: Arc<RPCState<DB>>) -> HashMap<ApiPaths, RpcModule<RPCState<DB>>>
where
    DB: Blockstore + Send + Sync + 'static,
{
    let mut modules = HashMap::default();
    for api_version in ApiPaths::value_variants() {
        modules.insert(*api_version, RpcModule::from_arc(state.clone()));
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
        db::MemoryDB, networks::NetworkChain, rpc::common::ShiftingVersion,
        tool::offline_server::server::offline_rpc_state,
    };
    use jsonrpsee::server::stop_channel;
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

    #[tokio::test(flavor = "multi_thread")]
    async fn test_rpc_server() {
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

        let handle = tokio::spawn(start_rpc(state, rpc_listener, stop_handle, None));

        // Send a few http requests

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

        // Send a few websocket requests

        let client = Client::from_url(
            format!("ws://{}:{}/", rpc_address.ip(), rpc_address.port())
                .parse()
                .unwrap(),
        );

        let response = super::methods::auth::AuthVerify::call(&client, (jwt_read,))
            .await
            .unwrap();
        assert_eq!(response, jwt_read_permissions);

        // Gracefully shutdown the RPC server
        shutdown_send.send(()).await.unwrap();
        shutdown_recv.recv().await;
        server_handle.stop().unwrap();
        handle.await.unwrap().unwrap();
    }
}
