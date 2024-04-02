// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

/// JSON-RPC API definitions

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
