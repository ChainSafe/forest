// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

/// JSON-RPC API definitions

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
    pub const STATE_MINER_AVAILABLE_BALANCE: &str = "Filecoin.StateMinerAvailableBalance";
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
    pub const STATE_MARKET_STORAGE_DEAL: &str = "Filecoin.StateMarketStorageDeal";
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
    pub const NET_VERSION: &str = "Filecoin.NetVersion";

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
    pub const ETH_SYNCING: &str = "Filecoin.EthSyncing";

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
                Self::BlockNumber(number) => format!("{:#x}", number),
                Self::BlockHash(hash, _require_canonical) => format!("{:#x}", hash.0),
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

    #[derive(Debug, Clone, Default)]
    pub struct EthSyncingResult {
        pub done_sync: bool,
        pub starting_block: i64,
        pub current_block: i64,
        pub highest_block: i64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(untagged)]
    pub enum EthSyncingResultLotusJson {
        DoneSync(bool),
        Syncing {
            #[serde(rename = "startingblock", with = "crate::lotus_json::hexify")]
            starting_block: i64,
            #[serde(rename = "currentblock", with = "crate::lotus_json::hexify")]
            current_block: i64,
            #[serde(rename = "highestblock", with = "crate::lotus_json::hexify")]
            highest_block: i64,
        },
    }

    impl HasLotusJson for EthSyncingResult {
        type LotusJson = EthSyncingResultLotusJson;

        #[cfg(test)]
        fn snapshots() -> Vec<(serde_json::Value, Self)> {
            vec![]
        }

        fn into_lotus_json(self) -> Self::LotusJson {
            match self {
                Self {
                    done_sync: false,
                    starting_block,
                    current_block,
                    highest_block,
                } => EthSyncingResultLotusJson::Syncing {
                    starting_block,
                    current_block,
                    highest_block,
                },
                _ => EthSyncingResultLotusJson::DoneSync(false),
            }
        }

        fn from_lotus_json(lotus_json: Self::LotusJson) -> Self {
            match lotus_json {
                EthSyncingResultLotusJson::DoneSync(syncing) => {
                    if syncing {
                        // Dangerous to panic here, log error instead.
                        tracing::error!("Invalid EthSyncingResultLotusJson: {syncing}");
                    }
                    Self {
                        done_sync: true,
                        ..Default::default()
                    }
                }
                EthSyncingResultLotusJson::Syncing {
                    starting_block,
                    current_block,
                    highest_block,
                } => Self {
                    done_sync: false,
                    starting_block,
                    current_block,
                    highest_block,
                },
            }
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
            assert_eq!(encoded, format!("\"{i:#x}\""));
            let decoded: GasPriceResult = serde_json::from_str(&encoded).unwrap();
            assert_eq!(r.0, decoded.0);
        }
    }
}
