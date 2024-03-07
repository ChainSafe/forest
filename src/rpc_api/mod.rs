// Copyright 2019-2024 ChainSafe Systems
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
use crate::rpc::CANCEL_METHOD_NAME;

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
    access.insert(chain_api::CHAIN_GET_PATH, Access::Read);
    access.insert(chain_api::CHAIN_HAS_OBJ, Access::Read);
    access.insert(chain_api::CHAIN_GET_BLOCK_MESSAGES, Access::Read);
    access.insert(chain_api::CHAIN_GET_TIPSET_BY_HEIGHT, Access::Read);
    access.insert(chain_api::CHAIN_GET_TIPSET_AFTER_HEIGHT, Access::Read);
    access.insert(chain_api::CHAIN_GET_GENESIS, Access::Read);
    access.insert(chain_api::CHAIN_HEAD, Access::Read);
    access.insert(chain_api::CHAIN_GET_BLOCK, Access::Read);
    access.insert(chain_api::CHAIN_GET_TIPSET, Access::Read);
    access.insert(chain_api::CHAIN_SET_HEAD, Access::Admin);
    access.insert(chain_api::CHAIN_GET_MIN_BASE_FEE, Access::Admin);
    access.insert(chain_api::CHAIN_GET_MESSAGES_IN_TIPSET, Access::Read);
    access.insert(chain_api::CHAIN_GET_PARENT_MESSAGES, Access::Read);
    access.insert(chain_api::CHAIN_NOTIFY, Access::Read);
    access.insert(chain_api::CHAIN_GET_PARENT_RECEIPTS, Access::Read);

    // Message Pool API
    access.insert(mpool_api::MPOOL_GET_NONCE, Access::Read);
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
    access.insert(wallet_api::WALLET_VALIDATE_ADDRESS, Access::Read);
    access.insert(wallet_api::WALLET_VERIFY, Access::Read);
    access.insert(wallet_api::WALLET_DELETE, Access::Write);

    // State API
    access.insert(state_api::STATE_CALL, Access::Read);
    access.insert(state_api::STATE_REPLAY, Access::Read);
    access.insert(state_api::STATE_GET_ACTOR, Access::Read);
    access.insert(state_api::STATE_MARKET_BALANCE, Access::Read);
    access.insert(state_api::STATE_MARKET_DEALS, Access::Read);
    access.insert(state_api::STATE_MINER_INFO, Access::Read);
    access.insert(state_api::MINER_GET_BASE_INFO, Access::Read);
    access.insert(state_api::STATE_MINER_ACTIVE_SECTORS, Access::Read);
    access.insert(state_api::STATE_MINER_FAULTS, Access::Read);
    access.insert(state_api::STATE_MINER_RECOVERIES, Access::Read);
    access.insert(state_api::STATE_MINER_POWER, Access::Read);
    access.insert(state_api::STATE_MINER_DEADLINES, Access::Read);
    access.insert(state_api::STATE_MINER_PROVING_DEADLINE, Access::Read);
    access.insert(state_api::STATE_GET_RECEIPT, Access::Read);
    access.insert(state_api::STATE_WAIT_MSG, Access::Read);
    access.insert(state_api::STATE_SEARCH_MSG, Access::Read);
    access.insert(state_api::STATE_SEARCH_MSG_LIMITED, Access::Read);
    access.insert(state_api::STATE_NETWORK_NAME, Access::Read);
    access.insert(state_api::STATE_NETWORK_VERSION, Access::Read);
    access.insert(state_api::STATE_ACCOUNT_KEY, Access::Read);
    access.insert(state_api::STATE_LOOKUP_ID, Access::Read);
    access.insert(state_api::STATE_FETCH_ROOT, Access::Read);
    access.insert(state_api::STATE_GET_RANDOMNESS_FROM_TICKETS, Access::Read);
    access.insert(state_api::STATE_GET_RANDOMNESS_FROM_BEACON, Access::Read);
    access.insert(state_api::STATE_READ_STATE, Access::Read);
    access.insert(state_api::STATE_CIRCULATING_SUPPLY, Access::Read);
    access.insert(state_api::STATE_SECTOR_GET_INFO, Access::Read);
    access.insert(state_api::STATE_LIST_MESSAGES, Access::Read);
    access.insert(state_api::STATE_LIST_MINERS, Access::Read);
    access.insert(state_api::STATE_MINER_SECTOR_COUNT, Access::Read);
    access.insert(state_api::STATE_VERIFIED_CLIENT_STATUS, Access::Read);
    access.insert(
        state_api::STATE_VM_CIRCULATING_SUPPLY_INTERNAL,
        Access::Read,
    );
    access.insert(state_api::MSIG_GET_AVAILABLE_BALANCE, Access::Read);
    access.insert(state_api::MSIG_GET_PENDING, Access::Read);

    // Gas API
    access.insert(gas_api::GAS_ESTIMATE_GAS_LIMIT, Access::Read);
    access.insert(gas_api::GAS_ESTIMATE_GAS_PREMIUM, Access::Read);
    access.insert(gas_api::GAS_ESTIMATE_FEE_CAP, Access::Read);
    access.insert(gas_api::GAS_ESTIMATE_MESSAGE_GAS, Access::Read);

    // Common API
    access.insert(common_api::VERSION, Access::Read);
    access.insert(common_api::SESSION, Access::Read);
    access.insert(common_api::SHUTDOWN, Access::Admin);
    access.insert(common_api::START_TIME, Access::Read);

    // Net API
    access.insert(net_api::NET_ADDRS_LISTEN, Access::Read);
    access.insert(net_api::NET_PEERS, Access::Read);
    access.insert(net_api::NET_LISTENING, Access::Read);
    access.insert(net_api::NET_INFO, Access::Read);
    access.insert(net_api::NET_CONNECT, Access::Write);
    access.insert(net_api::NET_DISCONNECT, Access::Write);
    access.insert(net_api::NET_AGENT_VERSION, Access::Read);
    access.insert(net_api::NET_AUTO_NAT_STATUS, Access::Read);

    // Node API
    access.insert(node_api::NODE_STATUS, Access::Read);

    // Eth API
    access.insert(eth_api::ETH_ACCOUNTS, Access::Read);
    access.insert(eth_api::ETH_BLOCK_NUMBER, Access::Read);
    access.insert(eth_api::ETH_CHAIN_ID, Access::Read);
    access.insert(eth_api::ETH_GAS_PRICE, Access::Read);
    access.insert(eth_api::ETH_GET_BALANCE, Access::Read);

    // Pubsub API
    access.insert(CANCEL_METHOD_NAME, Access::Read);

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
    use std::{path::PathBuf, sync::Arc};

    use super::data_types::ApiTipsetKey;
    #[cfg(test)]
    use crate::blocks::RawBlockHeader;
    use crate::blocks::Tipset;
    use crate::lotus_json::lotus_json_with_self;
    #[cfg(test)]
    use crate::lotus_json::{assert_all_snapshots, assert_unchanged_via_json};
    use crate::lotus_json::{HasLotusJson, LotusJson};
    use crate::shim::clock::ChainEpoch;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    pub const CHAIN_GET_MESSAGE: &str = "Filecoin.ChainGetMessage";

    pub const CHAIN_EXPORT: &str = "Filecoin.ChainExport";

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    pub struct ChainExportParams {
        pub epoch: ChainEpoch,
        pub recent_roots: i64,
        pub output_path: PathBuf,
        // this has to be LotusJson<T> so #[derive(JsonSchema)] works properly
        pub tipset_keys: LotusJson<ApiTipsetKey>,
        pub skip_checksum: bool,
        pub dry_run: bool,
    }

    lotus_json_with_self!(ChainExportParams);

    pub type ChainExportResult = Option<String>;

    pub const CHAIN_READ_OBJ: &str = "Filecoin.ChainReadObj";
    pub const CHAIN_HAS_OBJ: &str = "Filecoin.ChainHasObj";
    pub const CHAIN_GET_BLOCK_MESSAGES: &str = "Filecoin.ChainGetBlockMessages";
    pub const CHAIN_GET_TIPSET_BY_HEIGHT: &str = "Filecoin.ChainGetTipSetByHeight";
    pub const CHAIN_GET_TIPSET_AFTER_HEIGHT: &str = "Filecoin.ChainGetTipSetAfterHeight";
    pub const CHAIN_GET_GENESIS: &str = "Filecoin.ChainGetGenesis";
    pub const CHAIN_HEAD: &str = "Filecoin.ChainHead";
    pub const CHAIN_GET_BLOCK: &str = "Filecoin.ChainGetBlock";
    pub const CHAIN_GET_TIPSET: &str = "Filecoin.ChainGetTipSet";
    pub const CHAIN_GET_PATH: &str = "Filecoin.ChainGetPath";
    pub const CHAIN_SET_HEAD: &str = "Filecoin.ChainSetHead";
    pub const CHAIN_GET_MIN_BASE_FEE: &str = "Filecoin.ChainGetMinBaseFee";
    pub const CHAIN_GET_MESSAGES_IN_TIPSET: &str = "Filecoin.ChainGetMessagesInTipset";
    pub const CHAIN_GET_PARENT_MESSAGES: &str = "Filecoin.ChainGetParentMessages";
    pub const CHAIN_NOTIFY: &str = "Filecoin.ChainNotify";
    pub const CHAIN_GET_PARENT_RECEIPTS: &str = "Filecoin.ChainGetParentReceipts";

    #[derive(PartialEq, Debug, Serialize, Deserialize, Clone, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    pub enum PathChange<T = Arc<Tipset>> {
        Revert(T),
        Apply(T),
    }
    impl HasLotusJson for PathChange {
        type LotusJson = PathChange<LotusJson<Tipset>>;

        #[cfg(test)]
        fn snapshots() -> Vec<(serde_json::Value, Self)> {
            use serde_json::json;
            vec![(
                json!({
                    "revert": {
                        "Blocks": [
                            {
                                "BeaconEntries": null,
                                "ForkSignaling": 0,
                                "Height": 0,
                                "Messages": { "/": "baeaaaaa" },
                                "Miner": "f00",
                                "ParentBaseFee": "0",
                                "ParentMessageReceipts": { "/": "baeaaaaa" },
                                "ParentStateRoot": { "/":"baeaaaaa" },
                                "ParentWeight": "0",
                                "Parents": [{"/":"bafyreiaqpwbbyjo4a42saasj36kkrpv4tsherf2e7bvezkert2a7dhonoi"}],
                                "Timestamp": 0,
                                "WinPoStProof": null
                            }
                        ],
                        "Cids": [
                            { "/": "bafy2bzaceag62hjj3o43lf6oyeox3fvg5aqkgl5zagbwpjje3ajwg6yw4iixk" }
                        ],
                        "Height": 0
                    }
                }),
                Self::Revert(Arc::new(Tipset::from(RawBlockHeader::default()))),
            )]
        }

        fn into_lotus_json(self) -> Self::LotusJson {
            match self {
                PathChange::Revert(it) => PathChange::Revert(LotusJson(Tipset::clone(&it))),
                PathChange::Apply(it) => PathChange::Apply(LotusJson(Tipset::clone(&it))),
            }
        }

        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
            match lotus_json {
                PathChange::Revert(it) => PathChange::Revert(it.into_inner().into()),
                PathChange::Apply(it) => PathChange::Apply(it.into_inner().into()),
            }
        }
    }

    #[cfg(test)]
    impl<T> quickcheck::Arbitrary for PathChange<T>
    where
        T: quickcheck::Arbitrary,
    {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let inner = T::arbitrary(g);
            g.choose(&[PathChange::Apply(inner.clone()), PathChange::Revert(inner)])
                .unwrap()
                .clone()
        }
    }

    #[test]
    fn snapshots() {
        assert_all_snapshots::<PathChange>()
    }

    #[cfg(test)]
    quickcheck::quickcheck! {
        fn quickcheck(val: PathChange) -> () {
            assert_unchanged_via_json(val)
        }
    }
}

/// Message Pool API
pub mod mpool_api {
    pub const MPOOL_GET_NONCE: &str = "Filecoin.MpoolGetNonce";
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
    pub const WALLET_VALIDATE_ADDRESS: &str = "Filecoin.WalletValidateAddress";
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
    pub const STATE_MINER_INFO: &str = "Filecoin.StateMinerInfo";
    pub const MINER_GET_BASE_INFO: &str = "Filecoin.MinerGetBaseInfo";
    pub const STATE_MINER_FAULTS: &str = "Filecoin.StateMinerFaults";
    pub const STATE_MINER_RECOVERIES: &str = "Filecoin.StateMinerRecoveries";
    pub const STATE_MINER_POWER: &str = "Filecoin.StateMinerPower";
    pub const STATE_MINER_DEADLINES: &str = "Filecoin.StateMinerDeadlines";
    pub const STATE_MINER_PROVING_DEADLINE: &str = "Filecoin.StateMinerProvingDeadline";
    pub const STATE_GET_RECEIPT: &str = "Filecoin.StateGetReceipt";
    pub const STATE_WAIT_MSG: &str = "Filecoin.StateWaitMsg";
    pub const STATE_FETCH_ROOT: &str = "Filecoin.StateFetchRoot";
    pub const STATE_GET_RANDOMNESS_FROM_TICKETS: &str = "Filecoin.StateGetRandomnessFromTickets";
    pub const STATE_GET_RANDOMNESS_FROM_BEACON: &str = "Filecoin.StateGetRandomnessFromBeacon";
    pub const STATE_READ_STATE: &str = "Filecoin.StateReadState";
    pub const STATE_MINER_ACTIVE_SECTORS: &str = "Filecoin.StateMinerActiveSectors";
    pub const STATE_LOOKUP_ID: &str = "Filecoin.StateLookupID";
    pub const STATE_ACCOUNT_KEY: &str = "Filecoin.StateAccountKey";
    pub const STATE_CIRCULATING_SUPPLY: &str = "Filecoin.StateCirculatingSupply";
    pub const STATE_DECODE_PARAMS: &str = "Filecoin.StateDecodeParams";
    pub const STATE_SECTOR_GET_INFO: &str = "Filecoin.StateSectorGetInfo";
    pub const STATE_SEARCH_MSG: &str = "Filecoin.StateSearchMsg";
    pub const STATE_SEARCH_MSG_LIMITED: &str = "Filecoin.StateSearchMsgLimited";
    pub const STATE_LIST_MESSAGES: &str = "Filecoin.StateListMessages";
    pub const STATE_LIST_MINERS: &str = "Filecoin.StateListMiners";
    pub const STATE_MINER_SECTOR_COUNT: &str = "Filecoin.StateMinerSectorCount";
    pub const STATE_VERIFIED_CLIENT_STATUS: &str = "Filecoin.StateVerifiedClientStatus";
    pub const STATE_VM_CIRCULATING_SUPPLY_INTERNAL: &str =
        "Filecoin.StateVMCirculatingSupplyInternal";
    pub const MSIG_GET_AVAILABLE_BALANCE: &str = "Filecoin.MsigGetAvailableBalance";
    pub const MSIG_GET_PENDING: &str = "Filecoin.MsigGetPending";
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
    pub const NET_LISTENING: &str = "Filecoin.NetListening";

    pub const NET_INFO: &str = "Filecoin.NetInfo";
    pub const NET_CONNECT: &str = "Filecoin.NetConnect";
    pub const NET_DISCONNECT: &str = "Filecoin.NetDisconnect";
    pub const NET_AGENT_VERSION: &str = "Filecoin.NetAgentVersion";
    pub const NET_AUTO_NAT_STATUS: &str = "Filecoin.NetAutoNatStatus";

    #[derive(Debug, Default, Serialize, Deserialize, Clone)]
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

    #[derive(Debug, Default, Serialize, Deserialize, Clone)]
    #[serde(rename_all = "PascalCase")]
    pub struct NatStatusResult {
        pub reachability: i32,
        pub public_addrs: Option<Vec<String>>,
    }
    lotus_json_with_self!(NatStatusResult);

    impl NatStatusResult {
        // See <https://github.com/libp2p/go-libp2p/blob/164adb40fef9c19774eb5fe6d92afb95c67ba83c/core/network/network.go#L93>
        pub fn reachability_as_str(&self) -> &'static str {
            match self.reachability {
                0 => "Unknown",
                1 => "Public",
                2 => "Private",
                _ => "(unrecognized)",
            }
        }
    }

    impl From<libp2p::autonat::NatStatus> for NatStatusResult {
        fn from(nat: libp2p::autonat::NatStatus) -> Self {
            use libp2p::autonat::NatStatus;

            // See <https://github.com/libp2p/go-libp2p/blob/91e1025f04519a5560361b09dfccd4b5239e36e6/core/network/network.go#L77>
            let (reachability, public_addrs) = match &nat {
                NatStatus::Unknown => (0, None),
                NatStatus::Public(addr) => (1, Some(vec![addr.to_string()])),
                NatStatus::Private => (2, None),
            };

            NatStatusResult {
                reachability,
                public_addrs,
            }
        }
    }
}

/// Node API
pub mod node_api {
    pub const NODE_STATUS: &str = "Filecoin.NodeStatus";
    pub type NodeStatusResult = NodeStatus;

    use serde::{Deserialize, Serialize};

    use crate::lotus_json::lotus_json_with_self;

    #[derive(Debug, Serialize, Deserialize, Default, Clone)]
    pub struct NodeSyncStatus {
        pub epoch: u64,
        pub behind: u64,
    }

    #[derive(Debug, Serialize, Deserialize, Default, Clone)]
    pub struct NodePeerStatus {
        pub peers_to_publish_msgs: u32,
        pub peers_to_publish_blocks: u32,
    }

    #[derive(Debug, Serialize, Deserialize, Default, Clone)]
    pub struct NodeChainStatus {
        pub blocks_per_tipset_last_100: f64,
        pub blocks_per_tipset_last_finality: f64,
    }

    #[derive(Debug, Deserialize, Default, Serialize, Clone)]
    pub struct NodeStatus {
        pub sync_status: NodeSyncStatus,
        pub peer_status: NodePeerStatus,
        pub chain_status: NodeChainStatus,
    }

    lotus_json_with_self!(NodeStatus);
}

// Eth API
pub mod eth_api {
    use std::{fmt, str::FromStr};

    use cid::{
        multihash::{self, MultihashDigest},
        Cid,
    };
    use num_bigint;
    use serde::{Deserialize, Serialize};

    use crate::lotus_json::{lotus_json_with_self, HasLotusJson};
    use crate::shim::address::Address as FilecoinAddress;

    pub const ETH_ACCOUNTS: &str = "Filecoin.EthAccounts";
    pub const ETH_BLOCK_NUMBER: &str = "Filecoin.EthBlockNumber";
    pub const ETH_CHAIN_ID: &str = "Filecoin.EthChainId";
    pub const ETH_GAS_PRICE: &str = "Filecoin.EthGasPrice";
    pub const ETH_GET_BALANCE: &str = "Filecoin.EthGetBalance";

    const MASKED_ID_PREFIX: [u8; 12] = [0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

    #[derive(Debug, Deserialize, Serialize, Default, Clone)]
    pub struct GasPriceResult(#[serde(with = "crate::lotus_json::hexify")] pub num_bigint::BigInt);

    lotus_json_with_self!(GasPriceResult);

    #[derive(PartialEq, Debug, Deserialize, Serialize, Default, Clone)]
    pub struct BigInt(#[serde(with = "crate::lotus_json::hexify")] pub num_bigint::BigInt);

    lotus_json_with_self!(BigInt);

    #[derive(Debug, Deserialize, Serialize, Default, Clone)]
    pub struct Address(
        #[serde(with = "crate::lotus_json::hexify_bytes")] pub ethereum_types::Address,
    );

    lotus_json_with_self!(Address);

    impl Address {
        pub fn to_filecoin_address(&self) -> Result<FilecoinAddress, anyhow::Error> {
            if self.is_masked_id() {
                // This is a masked ID address.
                #[allow(clippy::indexing_slicing)]
                let bytes: [u8; 8] =
                    core::array::from_fn(|i| self.0.as_fixed_bytes()[MASKED_ID_PREFIX.len() + i]);
                Ok(FilecoinAddress::new_id(u64::from_be_bytes(bytes)))
            } else {
                // Otherwise, translate the address into an address controlled by the
                // Ethereum Address Manager.
                Ok(FilecoinAddress::new_delegated(
                    FilecoinAddress::ETHEREUM_ACCOUNT_MANAGER_ACTOR.id()?,
                    self.0.as_bytes(),
                )?)
            }
        }

        fn is_masked_id(&self) -> bool {
            self.0.as_bytes().starts_with(&MASKED_ID_PREFIX)
        }
    }

    impl FromStr for Address {
        type Err = anyhow::Error;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            Ok(Address(
                ethereum_types::Address::from_str(s).map_err(|e| anyhow::anyhow!("{e}"))?,
            ))
        }
    }

    #[derive(Default, Clone)]
    pub struct Hash(pub ethereum_types::H256);

    impl Hash {
        // Should ONLY be used for blocks and Filecoin messages. Eth transactions expect a different hashing scheme.
        pub fn to_cid(&self) -> cid::Cid {
            let mh = multihash::Code::Blake2b256.digest(self.0.as_bytes());
            Cid::new_v1(fvm_ipld_encoding::DAG_CBOR, mh)
        }
    }

    impl FromStr for Hash {
        type Err = anyhow::Error;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            Ok(Hash(ethereum_types::H256::from_str(s)?))
        }
    }

    #[derive(Default, Clone)]
    pub enum Predefined {
        Earliest,
        Pending,
        #[default]
        Latest,
    }

    impl fmt::Display for Predefined {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            let s = match self {
                Predefined::Earliest => "earliest",
                Predefined::Pending => "pending",
                Predefined::Latest => "latest",
            };
            write!(f, "{}", s)
        }
    }

    #[allow(dead_code)]
    #[derive(Clone)]
    pub enum BlockNumberOrHash {
        PredefinedBlock(Predefined),
        BlockNumber(i64),
        BlockHash(Hash, bool),
    }

    impl BlockNumberOrHash {
        pub fn from_predefined(predefined: Predefined) -> Self {
            Self::PredefinedBlock(predefined)
        }

        pub fn from_block_number(number: i64) -> Self {
            Self::BlockNumber(number)
        }
    }

    impl HasLotusJson for BlockNumberOrHash {
        type LotusJson = String;

        #[cfg(test)]
        fn snapshots() -> Vec<(serde_json::Value, Self)> {
            vec![]
        }

        fn into_lotus_json(self) -> Self::LotusJson {
            match self {
                Self::PredefinedBlock(predefined) => predefined.to_string(),
                Self::BlockNumber(number) => format!("0x{:x}", number),
                Self::BlockHash(hash, _require_canonical) => format!("0x{:x}", hash.0),
            }
        }

        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
            match lotus_json.as_str() {
                "earliest" => return Self::PredefinedBlock(Predefined::Earliest),
                "pending" => return Self::PredefinedBlock(Predefined::Pending),
                "latest" => return Self::PredefinedBlock(Predefined::Latest),
                _ => (),
            };

            #[allow(clippy::indexing_slicing)]
            if lotus_json.len() > 2 && &lotus_json[..2] == "0x" {
                if let Ok(number) = i64::from_str_radix(&lotus_json[2..], 16) {
                    return Self::BlockNumber(number);
                }
            }

            // Return some default value if we can't convert
            Self::PredefinedBlock(Predefined::Latest)
        }
    }

    #[cfg(test)]
    mod test {
        use super::*;
        use quickcheck_macros::quickcheck;

        #[quickcheck]
        fn gas_price_result_serde_roundtrip(i: u128) {
            let r = GasPriceResult(i.into());
            let encoded = serde_json::to_string(&r).unwrap();
            assert_eq!(encoded, format!("\"0x{i:x}\""));
            let decoded: GasPriceResult = serde_json::from_str(&encoded).unwrap();
            assert_eq!(r.0, decoded.0);
        }
    }
}
