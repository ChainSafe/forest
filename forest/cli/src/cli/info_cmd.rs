// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use chrono::{DateTime, Utc};
use clap::Subcommand;
use colored::*;
use forest_blocks::tipset_json::TipsetJson;
use forest_cli_shared::cli::CliOpts;
use forest_rpc_client::{
    chain_get_name, chain_head, node_ops::node_status, start_time, wallet_balance,
    wallet_default_address,
};
use forest_shim::econ::TokenAmount;
use forest_utils::misc::LoggingColor;

use fvm_shared::clock::EPOCH_DURATION_SECONDS;
use fvm_shared::{clock::ChainEpoch, BLOCKS_PER_EPOCH};
use num::BigInt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::Config;
use crate::cli::handle_rpc_err;
use crate::humantoken::{self, TokenAmountPretty};

#[derive(Debug, Subcommand)]
pub enum InfoCommand {
    Show,
}

#[derive(Debug)]
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
}

pub struct NodeInfoOutput {
    pub chain_status: ColoredString,
    pub network: ColoredString,
    pub uptime: ColoredString,
    pub health: ColoredString,
    pub wallet_address: ColoredString,
    pub wallet_balance: ColoredString,
}

#[derive(Debug, strum_macros::Display, PartialEq)]
pub enum SyncStatus {
    Ok,
    Slow,
    Behind,
}

pub fn fmt_duration(duration: Duration) -> String {
    let duration = humantime::format_duration(duration);
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

impl NodeStatusInfo {
    pub fn new(
        cur_duration: Duration,
        blocks_per_tipset_last_finality: f64,
        head: TipsetJson,
    ) -> anyhow::Result<NodeStatusInfo> {
        let ts = head.0.min_timestamp();
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

        let base_fee = head.0.min_ticket_block().parent_base_fee().clone();

        dbg!(&blocks_per_tipset_last_finality);

        let health = 100. * blocks_per_tipset_last_finality / BLOCKS_PER_EPOCH as f64;

        Ok(Self {
            behind: Duration::from_secs(behind),
            health,
            epoch: head.0.epoch(),
            base_fee,
            sync_status,
            start_time: Utc::now(),
            network: String::from("unknown"),
            default_wallet_address: None,
            default_wallet_address_balance: None,
        })
    }

    pub fn chain_status(&self) -> String {
        let base_fee_fmt = self.base_fee.pretty();

        let behind = format!("{}", humantime::format_duration(self.behind));
        format!(
            "[sync: {}! ({} behind)] [basefee: {base_fee_fmt}] [epoch: {}]",
            self.sync_status, behind, self.epoch
        )
    }
}

impl From<NodeStatusInfo> for NodeInfoOutput {
    fn from(node_status_info: NodeStatusInfo) -> NodeInfoOutput {
        let health = node_status_info.health;
        let uptime = (Utc::now() - node_status_info.start_time)
            .to_std()
            .expect("failed converting to std duration");
        let fmt_uptime = fmt_duration(uptime);
        let uptime = format!(
            "{fmt_uptime} (Started at: {})",
            node_status_info
                .start_time
                .with_timezone(&chrono::offset::Local)
        )
        .normal();

        let chain_status = node_status_info.chain_status().blue();
        let network = node_status_info.network.green();
        let wallet_address = node_status_info
            .default_wallet_address
            .clone()
            .unwrap_or("address not set".to_string())
            .bold();
        let health = {
            let h = format!("{health:.2}%\n\n");
            if health > 85. {
                h.green()
            } else if health > 50. {
                h.yellow()
            } else {
                h.red()
            }
        };

        let wallet_balance = match balance(node_status_info.default_wallet_address_balance) {
            Ok(bal) => bal.bold(),
            Err(err) => err.to_string().bold(),
        };

        NodeInfoOutput {
            chain_status,
            network,
            uptime,
            health,
            wallet_address,
            wallet_balance,
        }
    }
}

impl NodeInfoOutput {
    fn set_color(mut self, color: &LoggingColor) -> Self {
        if !color.coloring_enabled() {
            self.chain_status = self.chain_status.clear();
            self.network = self.network.clear();
            self.uptime = self.uptime.clear();
            self.health = self.health.clear();
            self.wallet_address = self.wallet_address.clear();
            self.wallet_balance = self.wallet_balance.clear();
        }

        self
    }
}

impl InfoCommand {
    pub async fn run(&self, config: Config, opts: &CliOpts) -> anyhow::Result<()> {
        let node_status = node_status(&config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;

        let head = chain_head(&config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;

        let network = chain_get_name((), &config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;

        let start_time = start_time(&config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;

        let cur_duration: Duration = SystemTime::now().duration_since(UNIX_EPOCH)?;
        let blocks_per_tipset_last_finality =
            node_status.chain_status.blocks_per_tipset_last_finality;

        let mut node_status_info =
            NodeStatusInfo::new(cur_duration, blocks_per_tipset_last_finality, head)?;

        let default_wallet_address = wallet_default_address((), &config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;

        let default_wallet_address_balance = if let Some(def_addr) = &default_wallet_address {
            let balance = wallet_balance((def_addr.clone(),), &config.client.rpc_token)
                .await
                .map_err(handle_rpc_err)?;
            Some(balance)
        } else {
            None
        };

        node_status_info.start_time = start_time;
        node_status_info.network = network;
        node_status_info.default_wallet_address = default_wallet_address.clone();
        node_status_info.default_wallet_address_balance = default_wallet_address_balance;

        let info_output = NodeInfoOutput::from(node_status_info).set_color(&opts.color);

        println!("Network: {}", info_output.network);
        println!("Uptime: {}", info_output.uptime);
        println!("Chain: {}", info_output.chain_status);
        println!("Chain health: {}", info_output.health);
        println!(
            "Default wallet address: {} [balance: {}]",
            &info_output.wallet_address, &info_output.wallet_balance
        );

        Ok(())
    }
}

fn balance(balance: Option<String>) -> anyhow::Result<String> {
    use crate::humantoken::TokenAmountPretty;
    if let Some(bal) = balance {
        let balance_token_amount = TokenAmount::from_atto(bal.parse::<BigInt>()?);
        Ok(format!("{:.4}", balance_token_amount.pretty()))
    } else {
        Err(anyhow::anyhow!("error fetching balance"))
    }
}

#[cfg(test)]
mod tests {
    use std::{str::FromStr, sync::Arc, time::Duration};

    use chrono::{DateTime, Utc};
    use colored::*;
    use forest_blocks::{tipset_json::TipsetJson, BlockHeader, Tipset};
    use forest_shim::{address::Address, econ::TokenAmount};
    use forest_utils::misc::LoggingColor;
    use fvm_shared::clock::EPOCH_DURATION_SECONDS;
    use quickcheck_macros::quickcheck;

    use super::{NodeStatusInfo, SyncStatus};
    use crate::cli::info_cmd::NodeInfoOutput;

    fn mock_tipset_at(seconds_since_unix_epoch: u64) -> Arc<Tipset> {
        let mock_header = BlockHeader::builder()
            .miner_address(Address::from_str("f2kmbjvz7vagl2z6pfrbjoggrkjofxspp7cqtw2zy").unwrap())
            .timestamp(seconds_since_unix_epoch)
            .build()
            .unwrap();
        let tipset = Tipset::from(&mock_header);

        Arc::new(tipset)
    }

    fn mock_node_status() -> NodeStatusInfo {
        NodeStatusInfo {
            behind: Duration::from_secs(0),
            health: 90.,
            epoch: i64::MAX,
            base_fee: TokenAmount::from_whole(1),
            sync_status: SyncStatus::Ok,
            start_time: DateTime::<Utc>::MIN_UTC,
            network: "calibnet".to_string(),
            default_wallet_address: Some("-".to_string()),
            default_wallet_address_balance: None,
        }
    }

    #[quickcheck]
    fn test_sync_status_ok(duration: Duration) {
        let status_result = NodeStatusInfo::new(
            duration,
            20.,
            TipsetJson(mock_tipset_at(
                duration.as_secs() + (EPOCH_DURATION_SECONDS as u64 * 3 / 2),
            )),
        );
        if let Ok(status) = status_result {
            assert_ne!(status.sync_status, SyncStatus::Slow);
            assert_ne!(status.sync_status, SyncStatus::Behind);
        }
    }

    #[quickcheck]
    fn test_sync_status_behind(duration: Duration) {
        let duration = duration + Duration::from_secs(300);
        let tipset = mock_tipset_at(duration.as_secs().saturating_sub(200));
        let node_status = NodeStatusInfo::new(duration, 20., TipsetJson(tipset)).unwrap();
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
        let node_status = NodeStatusInfo::new(duration, 20., TipsetJson(tipset)).unwrap();
        assert!(node_status.health.is_finite());
        assert_ne!(node_status.sync_status, SyncStatus::Behind);
        assert_ne!(node_status.sync_status, SyncStatus::Ok);
    }

    #[test]
    fn block_sync_timestamp() {
        let duration = Duration::from_secs(60);
        let tipset = mock_tipset_at(duration.as_secs() - 10);
        let node_status = NodeStatusInfo::new(duration, 20., TipsetJson(tipset)).unwrap();
        let node_info_output = NodeInfoOutput::from(node_status);
        assert!(node_info_output.chain_status.contains("10s behind"));
    }

    #[test]
    fn chain_status_test() {
        let cur_duration = Duration::from_secs(100_000);
        let tipset = mock_tipset_at(cur_duration.as_secs() - 59);
        let node_status = NodeStatusInfo::new(cur_duration, 20., TipsetJson(tipset)).unwrap();
        let node_info_output = NodeInfoOutput::from(node_status);
        let expected_status_fmt = "[sync: Slow! (59s behind)] [basefee: 0 atto FIL] [epoch: 0]";
        assert_eq!(
            expected_status_fmt.clear(),
            node_info_output
                .set_color(&LoggingColor::Never)
                .chain_status
        );

        let tipset = mock_tipset_at(cur_duration.as_secs() - 30000);
        let node_status = NodeStatusInfo::new(cur_duration, 20., TipsetJson(tipset)).unwrap();
        let node_info_output = NodeInfoOutput::from(node_status);

        let expected_status_fmt =
            "[sync: Behind! (8h 20m behind)] [basefee: 0 atto FIL] [epoch: 0]";
        assert_eq!(
            expected_status_fmt.clear(),
            node_info_output
                .set_color(&LoggingColor::Never)
                .chain_status
        );
    }

    #[test]
    fn test_node_info_formattting() {
        // no color tests
        let color = LoggingColor::Never;
        let node_status = mock_node_status();
        let chain_status = node_status.chain_status();
        let info = NodeInfoOutput::from(node_status).set_color(&color);
        assert_eq!(info.network, "calibnet".normal());
        assert_eq!(info.health, "90.00%\n\n".normal());
        assert_eq!(info.wallet_address, "-".normal());
        assert_eq!(info.chain_status, chain_status.normal());

        // with color tests
        let color = LoggingColor::Always;
        let node_status = mock_node_status();
        let chain_status = node_status.chain_status();
        let info = NodeInfoOutput::from(node_status).set_color(&color);
        assert_eq!(info.network, "calibnet".green());
        assert_eq!(info.health, "90.00%\n\n".green());
        assert_eq!(info.wallet_address, "-".bold());
        assert_eq!(info.chain_status, chain_status.blue());
    }
}
