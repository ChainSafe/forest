// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
//! In general, `forest` wants to support the same RPC messages as `lotus` (go
//! implementation of Filecoin).
//!
//! Follow the pattern set below, and don't forget to add an entry to the
//! [`ACCESS_MAP`] with the relevant permissions (consult the go implementation,
//! looking for a comment like `// perm: admin`)
//!
//! Future work:
//! - Have an `RpcEndpoint` trait.
use ahash::{HashMap, HashMapExt};
use once_cell::sync::Lazy;

pub mod data_types;

/// Access levels to be checked against JWT claims
pub enum Access {
    Admin,
    Sign,
    Write,
    Read,
}

/// Access mapping between method names and access levels
/// Checked against JWT claims on every request
pub static ACCESS_MAP: Lazy<HashMap<&str, Access>> = Lazy::new(|| {
    let mut access = HashMap::new();

    // Auth API
    access.insert(auth_api::AUTH_NEW, Access::Admin);
    access.insert(auth_api::AUTH_VERIFY, Access::Read);

    // Beacon API
    access.insert(beacon_api::BEACON_GET_ENTRY, Access::Read);

    // Chain API
    access.insert(chain_api::CHAIN_GET_MESSAGE, Access::Read);
    access.insert(chain_api::CHAIN_EXPORT, Access::Read);
    access.insert(chain_api::CHAIN_READ_OBJ, Access::Read);
    access.insert(chain_api::CHAIN_HAS_OBJ, Access::Read);
    access.insert(chain_api::CHAIN_GET_BLOCK_MESSAGES, Access::Read);
    access.insert(chain_api::CHAIN_GET_TIPSET_BY_HEIGHT, Access::Read);
    access.insert(chain_api::CHAIN_GET_GENESIS, Access::Read);
    access.insert(chain_api::CHAIN_HEAD, Access::Read);
    access.insert(chain_api::CHAIN_GET_BLOCK, Access::Read);
    access.insert(chain_api::CHAIN_GET_TIPSET, Access::Read);
    access.insert(chain_api::CHAIN_SET_HEAD, Access::Admin);
    access.insert(chain_api::CHAIN_GET_MIN_BASE_FEE, Access::Admin);

    // Message Pool API
    access.insert(mpool_api::MPOOL_PENDING, Access::Read);
    access.insert(mpool_api::MPOOL_PUSH, Access::Write);
    access.insert(mpool_api::MPOOL_PUSH_MESSAGE, Access::Sign);

    // Sync API
    access.insert(sync_api::SYNC_CHECK_BAD, Access::Read);
    access.insert(sync_api::SYNC_MARK_BAD, Access::Admin);
    access.insert(sync_api::SYNC_STATE, Access::Read);

    // Wallet API
    access.insert(wallet_api::WALLET_BALANCE, Access::Write);
    access.insert(wallet_api::WALLET_BALANCE, Access::Read);
    access.insert(wallet_api::WALLET_DEFAULT_ADDRESS, Access::Read);
    access.insert(wallet_api::WALLET_EXPORT, Access::Admin);
    access.insert(wallet_api::WALLET_HAS, Access::Write);
    access.insert(wallet_api::WALLET_IMPORT, Access::Admin);
    access.insert(wallet_api::WALLET_LIST, Access::Write);
    access.insert(wallet_api::WALLET_NEW, Access::Write);
    access.insert(wallet_api::WALLET_SET_DEFAULT, Access::Write);
    access.insert(wallet_api::WALLET_SIGN, Access::Sign);
    access.insert(wallet_api::WALLET_VERIFY, Access::Read);
    access.insert(wallet_api::WALLET_DELETE, Access::Write);

    // State API
    access.insert(state_api::STATE_CALL, Access::Read);
    access.insert(state_api::STATE_REPLAY, Access::Read);
    access.insert(state_api::STATE_GET_ACTOR, Access::Read);
    access.insert(state_api::STATE_MARKET_BALANCE, Access::Read);
    access.insert(state_api::STATE_MARKET_DEALS, Access::Read);
    access.insert(state_api::STATE_GET_RECEIPT, Access::Read);
    access.insert(state_api::STATE_WAIT_MSG, Access::Read);
    access.insert(state_api::STATE_NETWORK_NAME, Access::Read);
    access.insert(state_api::STATE_NETWORK_VERSION, Access::Read);
    access.insert(state_api::STATE_FETCH_ROOT, Access::Read);

    // Gas API
    access.insert(gas_api::GAS_ESTIMATE_GAS_LIMIT, Access::Read);
    access.insert(gas_api::GAS_ESTIMATE_GAS_PREMIUM, Access::Read);
    access.insert(gas_api::GAS_ESTIMATE_FEE_CAP, Access::Read);
    access.insert(gas_api::GAS_ESTIMATE_MESSAGE_GAS, Access::Read);

    // Common API
    access.insert(common_api::VERSION, Access::Read);
    access.insert(common_api::SHUTDOWN, Access::Admin);
    access.insert(common_api::START_TIME, Access::Read);

    // Net API
    access.insert(net_api::NET_ADDRS_LISTEN, Access::Read);
    access.insert(net_api::NET_PEERS, Access::Read);
    access.insert(net_api::NET_INFO, Access::Read);
    access.insert(net_api::NET_CONNECT, Access::Write);
    access.insert(net_api::NET_DISCONNECT, Access::Write);

    // DB API
    access.insert(db_api::DB_GC, Access::Write);

    // Progress API
    access.insert(progress_api::GET_PROGRESS, Access::Read);
    // Node API
    access.insert(node_api::NODE_STATUS, Access::Read);

    access
});

/// Checks an access enumeration against provided JWT claims
pub fn check_access(access: &Access, claims: &[String]) -> bool {
    match access {
        Access::Admin => claims.contains(&"admin".to_owned()),
        Access::Sign => claims.contains(&"sign".to_owned()),
        Access::Write => claims.contains(&"write".to_owned()),
        Access::Read => claims.contains(&"read".to_owned()),
    }
}

/// JSON-RPC API definitions

/// Authorization API
pub mod auth_api {
    use chrono::Duration;
    use serde::{Deserialize, Serialize};
    use serde_with::{serde_as, DurationSeconds};

    use crate::lotus_json::lotus_json_with_self;

    pub const AUTH_NEW: &str = "Filecoin.AuthNew";
    #[serde_as]
    #[derive(Deserialize, Serialize)]
    pub struct AuthNewParams {
        pub perms: Vec<String>,
        #[serde_as(as = "DurationSeconds<i64>")]
        pub token_exp: Duration,
    }
    lotus_json_with_self!(AuthNewParams);

    pub const AUTH_VERIFY: &str = "Filecoin.AuthVerify";
}

/// Beacon API
pub mod beacon_api {
    pub const BEACON_GET_ENTRY: &str = "Filecoin.BeaconGetEntry";
}

/// Chain API
pub mod chain_api {
    use std::path::PathBuf;

    use crate::blocks::TipsetKeys;
    use crate::lotus_json::lotus_json_with_self;
    use crate::shim::clock::ChainEpoch;
    use serde::{Deserialize, Serialize};

    pub const CHAIN_GET_MESSAGE: &str = "Filecoin.ChainGetMessage";

    pub const CHAIN_EXPORT: &str = "Filecoin.ChainExport";

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ChainExportParams {
        pub epoch: ChainEpoch,
        pub recent_roots: i64,
        pub output_path: PathBuf,
        #[serde(with = "crate::lotus_json")]
        pub tipset_keys: TipsetKeys,
        pub skip_checksum: bool,
        pub dry_run: bool,
    }

    lotus_json_with_self!(ChainExportParams);

    pub type ChainExportResult = Option<String>;

    pub const CHAIN_READ_OBJ: &str = "Filecoin.ChainReadObj";
    pub const CHAIN_HAS_OBJ: &str = "Filecoin.ChainHasObj";
    pub const CHAIN_GET_BLOCK_MESSAGES: &str = "Filecoin.ChainGetBlockMessages";
    pub const CHAIN_GET_TIPSET_BY_HEIGHT: &str = "Filecoin.ChainGetTipSetByHeight";
    pub const CHAIN_GET_GENESIS: &str = "Filecoin.ChainGetGenesis";
    pub const CHAIN_HEAD: &str = "Filecoin.ChainHead";
    pub const CHAIN_GET_BLOCK: &str = "Filecoin.ChainGetBlock";
    pub const CHAIN_GET_TIPSET: &str = "Filecoin.ChainGetTipSet";
    pub const CHAIN_SET_HEAD: &str = "Filecoin.ChainSetHead";
    pub const CHAIN_GET_MIN_BASE_FEE: &str = "Filecoin.ChainGetMinBaseFee";
    pub const CHAIN_GET_MESSAGES_IN_TIPSET: &str = "Filecoin.ChainGetMessagesInTipset";
    pub const CHAIN_GET_PARENT_MESSAGES: &str = "Filecoin.ChainGetParentMessages";
}

/// Message Pool API
pub mod mpool_api {
    pub const MPOOL_PENDING: &str = "Filecoin.MpoolPending";
    pub const MPOOL_PUSH: &str = "Filecoin.MpoolPush";
    pub const MPOOL_PUSH_MESSAGE: &str = "Filecoin.MpoolPushMessage";
}

/// Sync API
pub mod sync_api {
    pub const SYNC_CHECK_BAD: &str = "Filecoin.SyncCheckBad";
    pub const SYNC_MARK_BAD: &str = "Filecoin.SyncMarkBad";
    pub const SYNC_STATE: &str = "Filecoin.SyncState";
}

/// Wallet API
pub mod wallet_api {
    pub const WALLET_BALANCE: &str = "Filecoin.WalletBalance";
    pub const WALLET_DEFAULT_ADDRESS: &str = "Filecoin.WalletDefaultAddress";
    pub const WALLET_EXPORT: &str = "Filecoin.WalletExport";
    pub const WALLET_HAS: &str = "Filecoin.WalletHas";
    pub const WALLET_IMPORT: &str = "Filecoin.WalletImport";
    pub const WALLET_LIST: &str = "Filecoin.WalletList";
    pub const WALLET_NEW: &str = "Filecoin.WalletNew";
    pub const WALLET_SET_DEFAULT: &str = "Filecoin.WalletSetDefault";
    pub const WALLET_SIGN: &str = "Filecoin.WalletSign";
    pub const WALLET_VERIFY: &str = "Filecoin.WalletVerify";
    pub const WALLET_DELETE: &str = "Filecoin.WalletDelete";
}

/// State API
pub mod state_api {
    pub const STATE_CALL: &str = "Filecoin.StateCall";
    pub const STATE_REPLAY: &str = "Filecoin.StateReplay";
    pub const STATE_NETWORK_NAME: &str = "Filecoin.StateNetworkName";
    pub const STATE_NETWORK_VERSION: &str = "Filecoin.StateNetworkVersion";
    pub const STATE_GET_ACTOR: &str = "Filecoin.StateGetActor";
    pub const STATE_MARKET_BALANCE: &str = "Filecoin.StateMarketBalance";
    pub const STATE_MARKET_DEALS: &str = "Filecoin.StateMarketDeals";
    pub const STATE_GET_RECEIPT: &str = "Filecoin.StateGetReceipt";
    pub const STATE_WAIT_MSG: &str = "Filecoin.StateWaitMsg";
    pub const STATE_FETCH_ROOT: &str = "Filecoin.StateFetchRoot";
    pub const STATE_MINOR_POWER: &str = "Filecoin.StateMinerPower";
    pub const STATE_GET_RANDOMNESS_FROM_BEACON: &str = "Filecoin.StateGetRandomnessFromBeacon";
    pub const STATE_READ_STATE: &str = "Filecoin.StateReadState";
}

/// Gas API
pub mod gas_api {
    pub const GAS_ESTIMATE_FEE_CAP: &str = "Filecoin.GasEstimateFeeCap";
    pub const GAS_ESTIMATE_GAS_PREMIUM: &str = "Filecoin.GasEstimateGasPremium";
    pub const GAS_ESTIMATE_GAS_LIMIT: &str = "Filecoin.GasEstimateGasLimit";
    pub const GAS_ESTIMATE_MESSAGE_GAS: &str = "Filecoin.GasEstimateMessageGas";
}

/// Common API
pub mod common_api {
    pub const VERSION: &str = "Filecoin.Version";
    pub const SHUTDOWN: &str = "Filecoin.Shutdown";
    pub const START_TIME: &str = "Filecoin.StartTime";
    pub const DISCOVER: &str = "Filecoin.Discover";
    pub const SESSION: &str = "Filecoin.Session";
}

/// Net API
pub mod net_api {
    use serde::{Deserialize, Serialize};

    use crate::lotus_json::lotus_json_with_self;

    pub const NET_ADDRS_LISTEN: &str = "Filecoin.NetAddrsListen";

    pub const NET_PEERS: &str = "Filecoin.NetPeers";

    pub const NET_INFO: &str = "Filecoin.NetInfo";

    #[derive(Debug, Default, Serialize, Deserialize)]
    pub struct NetInfoResult {
        pub num_peers: usize,
        pub num_connections: u32,
        pub num_pending: u32,
        pub num_pending_incoming: u32,
        pub num_pending_outgoing: u32,
        pub num_established: u32,
    }
    lotus_json_with_self!(NetInfoResult);

    impl From<libp2p::swarm::NetworkInfo> for NetInfoResult {
        fn from(i: libp2p::swarm::NetworkInfo) -> Self {
            let counters = i.connection_counters();
            Self {
                num_peers: i.num_peers(),
                num_connections: counters.num_connections(),
                num_pending: counters.num_pending(),
                num_pending_incoming: counters.num_pending_incoming(),
                num_pending_outgoing: counters.num_pending_outgoing(),
                num_established: counters.num_established(),
            }
        }
    }

    pub const NET_CONNECT: &str = "Filecoin.NetConnect";
    pub const NET_DISCONNECT: &str = "Filecoin.NetDisconnect";
}

/// DB API
pub mod db_api {
    pub const DB_GC: &str = "Filecoin.DatabaseGarbageCollection";
}

/// Progress API
pub mod progress_api {
    use crate::lotus_json::lotus_json_with_self;
    use serde::{Deserialize, Serialize};

    pub const GET_PROGRESS: &str = "Filecoin.GetProgress";
    pub type GetProgressParams = (GetProgressType,);
    pub type GetProgressResult = (u64, u64);

    #[derive(Serialize, Deserialize)]
    pub enum GetProgressType {
        DatabaseGarbageCollection,
    }

    lotus_json_with_self!(GetProgressType);
}

/// Node API
pub mod node_api {
    pub const NODE_STATUS: &str = "Filecoin.NodeStatus";
    pub type NodeStatusResult = NodeStatus;

    use serde::{Deserialize, Serialize};

    use crate::lotus_json::lotus_json_with_self;

    #[derive(Debug, Serialize, Deserialize, Default)]
    pub struct NodeSyncStatus {
        pub epoch: u64,
        pub behind: u64,
    }

    #[derive(Debug, Serialize, Deserialize, Default)]
    pub struct NodePeerStatus {
        pub peers_to_publish_msgs: u32,
        pub peers_to_publish_blocks: u32,
    }

    #[derive(Debug, Serialize, Deserialize, Default)]
    pub struct NodeChainStatus {
        pub blocks_per_tipset_last_100: f64,
        pub blocks_per_tipset_last_finality: f64,
    }

    #[derive(Debug, Deserialize, Default, Serialize)]
    pub struct NodeStatus {
        pub sync_status: NodeSyncStatus,
        pub peer_status: NodePeerStatus,
        pub chain_status: NodeChainStatus,
    }

    lotus_json_with_self!(NodeStatus);
}
