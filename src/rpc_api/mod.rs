// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
/// In general, `forest` wants to support the same RPC messages as `lotus` (go
/// implementation of Filecoin). Current progress is tracked in
/// `ARCHITECTURE.md`.
///
/// Follow the pattern set below, and don't forget to add an entry to the
/// [`ACCESS_MAP`] with the relevant permissions (consult the go implementation,
/// looking for a comment like `// perm: admin`)
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
    access.insert(chain_api::CHAIN_GET_TIPSET_HASH, Access::Read);
    access.insert(chain_api::CHAIN_VALIDATE_TIPSET_CHECKPOINTS, Access::Read);
    access.insert(chain_api::CHAIN_GET_NAME, Access::Read);
    access.insert(chain_api::CHAIN_SET_HEAD, Access::Admin);

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

    // State API
    access.insert(state_api::STATE_CALL, Access::Read);
    access.insert(state_api::STATE_REPLAY, Access::Read);
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

    pub const AUTH_NEW: &str = "Filecoin.AuthNew";
    #[serde_as]
    #[derive(Deserialize, Serialize)]
    pub struct AuthNewParams {
        pub perms: Vec<String>,
        #[serde_as(as = "DurationSeconds<i64>")]
        pub token_exp: Duration,
    }
    pub type AuthNewResult = Vec<u8>;

    pub const AUTH_VERIFY: &str = "Filecoin.AuthVerify";
    pub type AuthVerifyParams = (String,);
    pub type AuthVerifyResult = Vec<String>;
}

/// Beacon API
pub mod beacon_api {
    use crate::beacon::json::BeaconEntryJson;
    use crate::shim::clock::ChainEpoch;

    pub const BEACON_GET_ENTRY: &str = "Filecoin.BeaconGetEntry";
    pub type BeaconGetEntryParams = (ChainEpoch,);
    pub type BeaconGetEntryResult = BeaconEntryJson;
}

/// Chain API
pub mod chain_api {
    use std::path::PathBuf;

    use crate::blocks::{
        header::json::BlockHeaderJson, tipset_json::TipsetJson, tipset_keys_json::TipsetKeysJson,
        TipsetKeys,
    };
    use crate::json::{cid::CidJson, message::json::MessageJson};
    use crate::shim::clock::ChainEpoch;
    use serde::{Deserialize, Serialize};

    use crate::rpc_api::data_types::BlockMessages;

    pub const CHAIN_GET_MESSAGE: &str = "Filecoin.ChainGetMessage";
    pub type ChainGetMessageParams = (CidJson,);
    pub type ChainGetMessageResult = MessageJson;

    pub const CHAIN_EXPORT: &str = "Filecoin.ChainExport";

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ChainExportParams {
        pub epoch: ChainEpoch,
        pub recent_roots: i64,
        pub output_path: PathBuf,
        pub tipset_keys: TipsetKeysJson,
        pub skip_checksum: bool,
        pub dry_run: bool,
    }

    pub type ChainExportResult = PathBuf;

    pub const CHAIN_READ_OBJ: &str = "Filecoin.ChainReadObj";
    pub type ChainReadObjParams = (CidJson,);
    pub type ChainReadObjResult = String;

    pub const CHAIN_HAS_OBJ: &str = "Filecoin.ChainHasObj";
    pub type ChainHasObjParams = (CidJson,);
    pub type ChainHasObjResult = bool;

    pub const CHAIN_GET_BLOCK_MESSAGES: &str = "Filecoin.ChainGetBlockMessages";
    pub type ChainGetBlockMessagesParams = (CidJson,);
    pub type ChainGetBlockMessagesResult = BlockMessages;

    pub const CHAIN_GET_TIPSET_BY_HEIGHT: &str = "Filecoin.ChainGetTipsetByHeight";
    pub type ChainGetTipsetByHeightParams = (ChainEpoch, TipsetKeys);
    pub type ChainGetTipsetByHeightResult = TipsetJson;

    pub const CHAIN_GET_GENESIS: &str = "Filecoin.ChainGetGenesis";
    #[allow(unused)] // https://github.com/ChainSafe/forest/issues/3029
    pub type ChainGetGenesisParams = ();
    pub type ChainGetGenesisResult = Option<TipsetJson>;

    pub const CHAIN_HEAD: &str = "Filecoin.ChainHead";
    #[allow(unused)] // https://github.com/ChainSafe/forest/issues/3029
    pub type ChainHeadParams = ();
    pub type ChainHeadResult = TipsetJson;

    pub const CHAIN_GET_BLOCK: &str = "Filecoin.ChainGetBlock";
    pub type ChainGetBlockParams = (CidJson,);
    pub type ChainGetBlockResult = BlockHeaderJson;

    pub const CHAIN_GET_TIPSET: &str = "Filecoin.ChainGetTipSet";
    pub type ChainGetTipSetParams = (TipsetKeysJson,);
    pub type ChainGetTipSetResult = TipsetJson;

    pub const CHAIN_GET_TIPSET_HASH: &str = "Filecoin.ChainGetTipSetHash";
    pub type ChainGetTipSetHashParams = (TipsetKeysJson,);
    pub type ChainGetTipSetHashResult = String;

    pub const CHAIN_VALIDATE_TIPSET_CHECKPOINTS: &str = "Filecoin.ChainValidateTipSetCheckpoints";
    pub type ChainValidateTipSetCheckpointsParams = ();
    pub type ChainValidateTipSetCheckpointsResult = String;

    pub const CHAIN_GET_NAME: &str = "Filecoin.ChainGetName";
    pub type ChainGetNameParams = ();
    pub type ChainGetNameResult = String;

    pub const CHAIN_SET_HEAD: &str = "Filecoin.ChainSetHead";
    pub type ChainSetHeadParams = (TipsetKeys,);
    pub type ChainSetHeadResult = ();
}

/// Message Pool API
pub mod mpool_api {
    use crate::json::{
        cid::{vec::CidJsonVec, CidJson},
        message::json::MessageJson,
        signed_message::json::SignedMessageJson,
    };
    use crate::message::SignedMessage;

    use crate::rpc_api::data_types::MessageSendSpec;

    pub const MPOOL_PENDING: &str = "Filecoin.MpoolPending";
    pub type MpoolPendingParams = (CidJsonVec,);
    pub type MpoolPendingResult = Vec<SignedMessage>;

    pub const MPOOL_PUSH: &str = "Filecoin.MpoolPush";
    pub type MpoolPushParams = (SignedMessageJson,);
    pub type MpoolPushResult = CidJson;

    pub const MPOOL_PUSH_MESSAGE: &str = "Filecoin.MpoolPushMessage";
    pub type MpoolPushMessageParams = (MessageJson, Option<MessageSendSpec>);
    pub type MpoolPushMessageResult = SignedMessageJson;
}

/// Sync API
pub mod sync_api {
    use crate::json::cid::CidJson;

    use crate::rpc_api::data_types::RPCSyncState;

    pub const SYNC_CHECK_BAD: &str = "Filecoin.SyncCheckBad";
    pub type SyncCheckBadParams = (CidJson,);
    pub type SyncCheckBadResult = String;

    pub const SYNC_MARK_BAD: &str = "Filecoin.SyncMarkBad";
    pub type SyncMarkBadParams = (CidJson,);
    pub type SyncMarkBadResult = ();

    pub const SYNC_STATE: &str = "Filecoin.SyncState";
    pub type SyncStateParams = ();
    pub type SyncStateResult = RPCSyncState;
}

/// Wallet API
pub mod wallet_api {
    use crate::json::{
        address::json::AddressJson,
        signature::json::{signature_type::SignatureTypeJson, SignatureJson},
    };
    use crate::key_management::json::KeyInfoJson;

    pub const WALLET_BALANCE: &str = "Filecoin.WalletBalance";
    pub type WalletBalanceParams = (String,);
    pub type WalletBalanceResult = String;

    pub const WALLET_DEFAULT_ADDRESS: &str = "Filecoin.WalletDefaultAddress";
    pub type WalletDefaultAddressParams = ();
    pub type WalletDefaultAddressResult = Option<String>;

    pub const WALLET_EXPORT: &str = "Filecoin.WalletExport";
    pub type WalletExportParams = (String,);
    pub type WalletExportResult = KeyInfoJson;

    pub const WALLET_HAS: &str = "Filecoin.WalletHas";
    pub type WalletHasParams = (String,);
    pub type WalletHasResult = bool;

    pub const WALLET_IMPORT: &str = "Filecoin.WalletImport";
    pub type WalletImportParams = Vec<KeyInfoJson>;
    pub type WalletImportResult = String;

    pub const WALLET_LIST: &str = "Filecoin.WalletList";
    pub type WalletListParams = ();
    pub type WalletListResult = Vec<AddressJson>;

    pub const WALLET_NEW: &str = "Filecoin.WalletNew";
    pub type WalletNewParams = (SignatureTypeJson,);
    pub type WalletNewResult = String;

    pub const WALLET_SET_DEFAULT: &str = "Filecoin.WalletSetDefault";
    pub type WalletSetDefaultParams = (AddressJson,);
    pub type WalletSetDefaultResult = ();

    pub const WALLET_SIGN: &str = "Filecoin.WalletSign";
    pub type WalletSignParams = (AddressJson, Vec<u8>);
    pub type WalletSignResult = SignatureJson;

    pub const WALLET_VERIFY: &str = "Filecoin.WalletVerify";
    pub type WalletVerifyParams = (AddressJson, Vec<u8>, SignatureJson);
    pub type WalletVerifyResult = bool;
}

/// State API
pub mod state_api {
    use crate::blocks::tipset_keys_json::TipsetKeysJson;
    use crate::json::{
        address::json::AddressJson, cid::CidJson, message::json::MessageJson,
        message_receipt::json::ReceiptJson,
    };
    use crate::shim::version::NetworkVersion;
    use crate::state_manager::{InvocResult, MarketBalance};
    use ahash::HashMap;

    use crate::rpc_api::data_types::{MarketDeal, MessageLookup};

    pub const STATE_CALL: &str = "Filecoin.StateCall";
    pub type StateCallParams = (MessageJson, TipsetKeysJson);
    pub type StateCallResult = InvocResult;

    pub const STATE_REPLAY: &str = "Filecoin.StateReplay";
    pub type StateReplayParams = (CidJson, TipsetKeysJson);
    pub type StateReplayResult = InvocResult;

    pub const STATE_NETWORK_NAME: &str = "Filecoin.StateNetworkName";
    #[allow(unused)] // https://github.com/ChainSafe/forest/issues/3029
    pub type StateNetworkNameParams = ();
    pub type StateNetworkNameResult = String;

    pub const STATE_NETWORK_VERSION: &str = "Filecoin.StateNetworkVersion";
    pub type StateNetworkVersionParams = (TipsetKeysJson,);
    pub type StateNetworkVersionResult = NetworkVersion;

    pub const STATE_MARKET_BALANCE: &str = "Filecoin.StateMarketBalance";
    pub type StateMarketBalanceParams = (AddressJson, TipsetKeysJson);
    pub type StateMarketBalanceResult = MarketBalance;

    pub const STATE_MARKET_DEALS: &str = "Filecoin.StateMarketDeals";
    pub type StateMarketDealsParams = (TipsetKeysJson,);
    pub type StateMarketDealsResult = HashMap<String, MarketDeal>;

    pub const STATE_GET_RECEIPT: &str = "Filecoin.StateGetReceipt";
    pub type StateGetReceiptParams = (CidJson, TipsetKeysJson);
    pub type StateGetReceiptResult = ReceiptJson;

    pub const STATE_WAIT_MSG: &str = "Filecoin.StateWaitMsg";
    pub type StateWaitMsgParams = (CidJson, i64);
    pub type StateWaitMsgResult = MessageLookup;

    pub const STATE_FETCH_ROOT: &str = "Filecoin.StateFetchRoot";
    pub type StateFetchRootParams = (CidJson,);
    pub type StateFetchRootResult = String;
}

/// Gas API
pub mod gas_api {
    use crate::blocks::tipset_keys_json::TipsetKeysJson;
    use crate::json::{address::json::AddressJson, message::json::MessageJson};

    use crate::rpc_api::data_types::MessageSendSpec;

    pub const GAS_ESTIMATE_FEE_CAP: &str = "Filecoin.GasEstimateFeeCap";
    pub type GasEstimateFeeCapParams = (MessageJson, i64, TipsetKeysJson);
    pub type GasEstimateFeeCapResult = String;

    pub const GAS_ESTIMATE_GAS_PREMIUM: &str = "Filecoin.GasEstimateGasPremium";
    pub type GasEstimateGasPremiumParams = (u64, AddressJson, i64, TipsetKeysJson);
    pub type GasEstimateGasPremiumResult = String;

    pub const GAS_ESTIMATE_GAS_LIMIT: &str = "Filecoin.GasEstimateGasLimit";
    pub type GasEstimateGasLimitParams = (MessageJson, TipsetKeysJson);
    pub type GasEstimateGasLimitResult = i64;

    pub const GAS_ESTIMATE_MESSAGE_GAS: &str = "Filecoin.GasEstimateMessageGas";
    pub type GasEstimateMessageGasParams = (MessageJson, Option<MessageSendSpec>, TipsetKeysJson);
    pub type GasEstimateMessageGasResult = MessageJson;
}

/// Common API
pub mod common_api {
    use chrono::Utc;

    use super::data_types::APIVersion;

    pub const VERSION: &str = "Filecoin.Version";
    pub type VersionParams = ();
    pub type VersionResult = APIVersion;

    pub const SHUTDOWN: &str = "Filecoin.Shutdown";
    pub type ShutdownParams = ();
    pub type ShutdownResult = ();

    pub const START_TIME: &str = "Filecoin.StartTime";
    #[allow(unused)] // https://github.com/ChainSafe/forest/issues/3029
    pub type StartTimeParams = ();
    pub type StartTimeResult = chrono::DateTime<Utc>;
}

/// Net API
pub mod net_api {
    use crate::rpc_api::data_types::AddrInfo;

    pub const NET_ADDRS_LISTEN: &str = "Filecoin.NetAddrsListen";
    pub type NetAddrsListenParams = ();
    pub type NetAddrsListenResult = AddrInfo;

    pub const NET_PEERS: &str = "Filecoin.NetPeers";
    pub type NetPeersParams = ();
    pub type NetPeersResult = Vec<AddrInfo>;

    pub const NET_CONNECT: &str = "Filecoin.NetConnect";
    pub type NetConnectParams = (AddrInfo,);
    pub type NetConnectResult = ();

    pub const NET_DISCONNECT: &str = "Filecoin.NetDisconnect";
    pub type NetDisconnectParams = (String,);
    pub type NetDisconnectResult = ();
}

/// DB API
pub mod db_api {
    pub const DB_GC: &str = "Filecoin.DatabaseGarbageCollection";
    pub type DBGCParams = ();
    pub type DBGCResult = ();
}

/// Progress API
pub mod progress_api {
    use serde::{Deserialize, Serialize};

    pub const GET_PROGRESS: &str = "Filecoin.GetProgress";
    pub type GetProgressParams = (GetProgressType,);
    pub type GetProgressResult = (u64, u64);

    #[derive(Serialize, Deserialize)]
    pub enum GetProgressType {
        SnapshotExport,
        DatabaseGarbageCollection,
    }
}

/// Node API
pub mod node_api {
    pub const NODE_STATUS: &str = "Filecoin.NodeStatus";
    pub type NodeStatusParams = ();
    pub type NodeStatusResult = NodeStatus;

    use serde::{Deserialize, Serialize};

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
}
