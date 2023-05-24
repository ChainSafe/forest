// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use ahash::HashSet;
use chrono::Utc;
use cid::Cid;
use fil_actor_interface::market::{DealProposal, DealState};
use forest_beacon::{Beacon, BeaconSchedule};
use forest_blocks::{tipset_keys_json::TipsetKeysJson, Tipset};
use forest_chain::ChainStore;
use forest_chain_sync::{BadBlockCache, SyncState};
use forest_ipld::json::IpldJson;
use forest_json::{cid::CidJson, message_receipt::json::ReceiptJson, token_amount::json};
use forest_key_management::KeyStore;
pub use forest_libp2p::{Multiaddr, Protocol};
use forest_libp2p::{Multihash, NetworkMessage};
use forest_message::signed_message::SignedMessage;
use forest_message_pool::{MessagePool, MpoolRpcProvider};
use forest_shim::{econ::TokenAmount, message::Message};
use forest_state_manager::StateManager;
use fvm_ipld_blockstore::Blockstore;
use jsonrpc_v2::{MapRouter as JsonRpcMapRouter, Server as JsonRpcServer};
use parking_lot::RwLock as SyncRwLock;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// This is where you store persistent data, or at least access to stateful
/// data.
pub struct RPCState<DB, B>
where
    DB: Blockstore,
    B: Beacon,
{
    pub keystore: Arc<RwLock<KeyStore>>,
    pub chain_store: Arc<ChainStore<DB>>,
    pub state_manager: Arc<StateManager<DB>>,
    pub mpool: Arc<MessagePool<MpoolRpcProvider<DB>>>,
    pub bad_blocks: Arc<BadBlockCache>,
    pub sync_state: Arc<SyncRwLock<SyncState>>,
    pub network_send: flume::Sender<NetworkMessage>,
    pub network_name: String,
    pub start_time: chrono::DateTime<Utc>,
    pub new_mined_block_tx: flume::Sender<Arc<Tipset>>,
    pub beacon: Arc<BeaconSchedule<B>>,
    pub gc_event_tx: flume::Sender<flume::Sender<anyhow::Result<()>>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RPCSyncState {
    #[serde(rename = "ActiveSyncs")]
    pub active_syncs: Vec<SyncState>,
}

pub type JsonRpcServerState = Arc<JsonRpcServer<JsonRpcMapRouter>>;

// Chain API
#[derive(Serialize, Deserialize)]
pub struct BlockMessages {
    #[serde(rename = "BlsMessages", with = "forest_json::message::json::vec")]
    pub bls_msg: Vec<Message>,
    #[serde(
        rename = "SecpkMessages",
        with = "forest_json::signed_message::json::vec"
    )]
    pub secp_msg: Vec<SignedMessage>,
    #[serde(rename = "Cids", with = "forest_json::cid::vec")]
    pub cids: Vec<Cid>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MessageSendSpec {
    #[serde(with = "json")]
    max_fee: TokenAmount,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MarketDeal {
    pub proposal: DealProposal,
    pub state: DealState,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MessageLookup {
    pub receipt: ReceiptJson,
    #[serde(rename = "TipSet")]
    pub tipset: TipsetKeysJson,
    pub height: i64,
    pub message: CidJson,
    pub return_dec: IpldJson,
}

// Net API
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AddrInfo {
    #[serde(rename = "ID")]
    pub id: String,
    pub addrs: HashSet<Multiaddr>,
}

#[derive(Serialize, Deserialize)]
pub struct PeerID {
    pub multihash: Multihash,
}

/// Represents the current version of the API.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct APIVersion {
    pub version: String,
    pub api_version: Version,
    pub block_delay: u64,
}

/// Integer based value on version information. Highest order bits for Major,
/// Mid order for Minor and lowest for Patch.
#[derive(Serialize, Deserialize)]
pub struct Version(u32);

impl Version {
    pub const fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self((major as u32) << 16 | (minor as u32) << 8 | (patch as u32))
    }
}

pub mod node_api {
    use std::time::Duration;

    use chrono::{DateTime, Utc};
    use colored::*;
    use fil_actor_interface::EPOCH_DURATION_SECONDS;
    use forest_blocks::tipset_json::TipsetJson;
    use forest_json::token_amount::json;
    use forest_shim::econ::TokenAmount;
    use forest_utils::io::parser::{format_balance_string, FormattingMode};
    use fvm_shared::{clock::ChainEpoch, BLOCKS_PER_EPOCH};
    use humantime::format_duration;
    use serde::{Deserialize, Serialize};
    #[derive(Debug, Serialize, Deserialize, Default)]
    struct NodeSyncStatus {
        epoch: u64,
        behind: u64,
    }

    #[derive(Debug, Serialize, Deserialize, Default)]
    struct NodePeerStatus {
        peers_to_publish_msgs: u32,
        peers_to_publish_blocks: u32,
    }

    #[derive(Debug, Serialize, Deserialize, Default)]
    struct NodeChainStatus {
        blocks_per_tipset_last_100: f64,
        blocks_per_tipset_last_finality: f64,
    }

    #[derive(Debug, Deserialize, Default, Serialize)]
    pub struct NodeStatus {
        sync_status: NodeSyncStatus,
        peer_status: NodePeerStatus,
        chain_status: NodeChainStatus,
    }

    #[derive(Debug, Deserialize, Serialize)]
    pub struct NodeStatusInfo {
        /// duration in seconds of how far behind the node is with respect to
        /// syncing to head
        pub behind: Duration,
        /// Chain health is the percentage denoting how close we are to having
        /// an average of 5 blocks per tipset in the last couple of
        /// hours. The number of blocks per tipset is non-deterministic
        /// but averaging at 5 is considered healthy.
        pub health: f64,
        /// epoch the node is currently at
        pub epoch: ChainEpoch,
        /// base fee
        #[serde(with = "json")]
        pub base_fee: TokenAmount,
        /// sync status information
        pub sync_status: SyncStatus,
        /// Start time of the node
        pub start_time: DateTime<Utc>,
        /// Current network the node is running on
        pub network: String,
        /// Default wallet address selected.
        pub default_wallet_address: Option<String>,
        /// Default wallet address balance
        pub default_wallet_address_balance: Option<String>,
        /// misc node information
        pub node_sync_status: NodeStatus,
    }

    pub struct NodeInfoOutput {
        pub chain_status: ColoredString,
        pub network: ColoredString,
        pub uptime: ColoredString,
        pub health: ColoredString,
        pub wallet_address: ColoredString,
    }

    #[derive(Debug, strum_macros::Display, PartialEq, Deserialize, Serialize)]
    pub enum SyncStatus {
        Ok,
        Slow,
        Behind,
    }

    impl NodeStatusInfo {
        pub fn new(
            cur_duration: Duration,
            tipsets: Vec<TipsetJson>,
            chain_finality: usize,
        ) -> anyhow::Result<NodeStatusInfo> {
            let head = tipsets
                .get(0)
                .map(|ts| ts.0.clone())
                .ok_or(anyhow::anyhow!("head tipset not found"))?;
            let num_tipsets = tipsets.len().max(chain_finality);
            let block_count: usize = tipsets.iter().map(|s| s.0.blocks().len()).sum();
            let epoch = head.epoch();
            let ts = head.min_timestamp();
            let cur_duration_secs = cur_duration.as_secs();
            let behind = if ts <= cur_duration_secs + 1 {
                cur_duration_secs.saturating_sub(ts)
            } else {
                anyhow::bail!(
                "System time should not be behind tipset timestamp, please sync the system clock."
            );
            };

            let sync_status = if behind < EPOCH_DURATION_SECONDS as u64 * 3 / 2 {
                // within 1.5 epochs
                SyncStatus::Ok
            } else if behind < EPOCH_DURATION_SECONDS as u64 * 5 {
                // within 5 epochs
                SyncStatus::Slow
            } else {
                SyncStatus::Behind
            };

            let base_fee = head.min_ticket_block().parent_base_fee().clone();

            let health =
                (100 * block_count) as f64 / (num_tipsets * BLOCKS_PER_EPOCH as usize) as f64;

            Ok(Self {
                behind: Duration::from_secs(behind),
                health,
                epoch,
                base_fee,
                sync_status,
                start_time: Utc::now(),
                network: String::from("unknown"),
                default_wallet_address: None,
                default_wallet_address_balance: None,
                node_sync_status: NodeStatus::default(),
            })
        }

        pub fn display(&self, color: &forest_utils::misc::LoggingColor) -> NodeInfoOutput {
            let NodeStatusInfo { health, .. } = self;

            let use_color = color.coloring_enabled();
            let uptime = (Utc::now() - self.start_time)
                .to_std()
                .expect("failed converting to std duration");
            let fmt_uptime = fmt_duration(uptime);
            let uptime = format!(
                "{fmt_uptime} (Started at: {})",
                self.start_time.with_timezone(&chrono::offset::Local)
            )
            .normal();

            let chain_status = self.chain_status().blue();
            let network = self.network.green();
            let wallet_address = self
                .default_wallet_address
                .clone()
                .unwrap_or("address not set".to_string())
                .bold();
            let health = {
                let s = format!("{health:.2}%\n\n");
                if *health > 85. {
                    s.green()
                } else if *health > 50. {
                    s.yellow()
                } else {
                    s.red()
                }
            };

            if !use_color {
                NodeInfoOutput {
                    chain_status: chain_status.clear(),
                    network: network.clear(),
                    uptime: uptime.clear(),
                    health: health.clear(),
                    wallet_address: wallet_address.clear(),
                }
            } else {
                NodeInfoOutput {
                    chain_status,
                    network,
                    uptime,
                    health,
                    wallet_address,
                }
            }
        }

        pub fn chain_status(&self) -> String {
            let base_fee =
                format_balance_string(self.base_fee.clone(), FormattingMode::NotExactNotFixed)
                    .unwrap_or("OutOfBounds".to_string());
            let behind = format!("{}", humantime::format_duration(self.behind));
            format!(
                "[sync: {}! ({} behind)] [basefee: {base_fee}] [epoch: {}]",
                self.sync_status, behind, self.epoch
            )
        }
    }
    fn fmt_duration(duration: Duration) -> String {
        let duration = format_duration(duration);
        let duration = duration.to_string();
        let duration = duration.split(' ');
        let format_duration = duration
            .filter(|s| !s.ends_with("us"))
            .filter(|s| !s.ends_with("ns"))
            .filter(|s| !s.ends_with("ms"))
            .map(|s| s.to_string());
        let format_duration: Vec<String> = format_duration.collect();
        format_duration.join(" ")
    }

    #[cfg(test)]
    mod tests {
        use std::{str::FromStr, sync::Arc, time::Duration};

        use chrono::{DateTime, Utc};
        use colored::*;
        use forest_blocks::{tipset_json::TipsetJson, BlockHeader, Tipset};
        // use forest_cli_shared::logger::LoggingColor;
        // use forest_rpc_api::node_api::NodeStatusInfo;
        use forest_shim::{address::Address, econ::TokenAmount};
        use forest_utils::misc::LoggingColor;
        use fvm_shared::clock::EPOCH_DURATION_SECONDS;
        use quickcheck_macros::quickcheck;

        use super::{NodeStatus, NodeStatusInfo, SyncStatus};
        // use crate::cli::info_cmd::SyncStatus;

        const CHAIN_FINALITY: usize = 900;

        fn mock_tipset_at(seconds_since_unix_epoch: u64) -> Arc<Tipset> {
            let mock_header = BlockHeader::builder()
                .miner_address(
                    Address::from_str("f2kmbjvz7vagl2z6pfrbjoggrkjofxspp7cqtw2zy").unwrap(),
                )
                .timestamp(seconds_since_unix_epoch)
                .build()
                .unwrap();
            let tipset = Tipset::from(&mock_header);

            Arc::new(tipset)
        }

        #[quickcheck]
        fn test_sync_status_ok(tipsets: Vec<Arc<Tipset>>) {
            let tipsets = tipsets.iter().map(|ts| TipsetJson(ts.clone())).collect();
            let status_result =
                NodeStatusInfo::new(Duration::from_secs(0), tipsets, CHAIN_FINALITY);
            if let Ok(status) = status_result {
                assert_ne!(status.sync_status, SyncStatus::Slow);
                assert_ne!(status.sync_status, SyncStatus::Behind);
            }
        }

        #[quickcheck]
        fn test_sync_status_behind(duration: Duration) {
            let duration = duration + Duration::from_secs(300);
            let tipset = mock_tipset_at(duration.as_secs().saturating_sub(200));
            let node_status =
                NodeStatusInfo::new(duration, vec![TipsetJson(tipset)], CHAIN_FINALITY).unwrap();
            assert!(node_status.health.is_finite());
            assert_ne!(node_status.sync_status, SyncStatus::Ok);
            assert_ne!(node_status.sync_status, SyncStatus::Slow);
        }

        #[quickcheck]
        fn test_sync_status_slow(duration: Duration) {
            let duration = duration + Duration::from_secs(300);
            let tipset = mock_tipset_at(
                duration
                    .as_secs()
                    .saturating_sub(EPOCH_DURATION_SECONDS as u64 * 4),
            );
            let node_status =
                NodeStatusInfo::new(duration, vec![TipsetJson(tipset)], CHAIN_FINALITY).unwrap();
            assert!(node_status.health.is_finite());
            assert_ne!(node_status.sync_status, SyncStatus::Behind);
            assert_ne!(node_status.sync_status, SyncStatus::Ok);
        }

        #[test]
        fn block_sync_timestamp() {
            let color = LoggingColor::Never;
            let duration = Duration::from_secs(60);
            let tipset = mock_tipset_at(duration.as_secs() - 10);
            let node_status =
                NodeStatusInfo::new(duration, vec![TipsetJson(tipset)], CHAIN_FINALITY).unwrap();
            let a = node_status.display(&color);
            assert!(a.chain_status.contains("10s behind"));
        }
    }
}
