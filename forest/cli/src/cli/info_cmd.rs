// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Utc};
use clap::Subcommand;
use colored::*;
use forest_blocks::tipset_json::TipsetJson;
use forest_cli_shared::{cli::CliOpts, logger::LoggingColor};
use forest_rpc_client::{
    chain_get_name, chain_get_tipsets_finality, start_time, wallet_default_address,
};
use forest_shim::econ::TokenAmount;
use forest_utils::io::parser::{format_balance_string, FormattingMode};
use fvm_shared::{
    clock::{ChainEpoch, EPOCH_DURATION_SECONDS},
    BLOCKS_PER_EPOCH,
};
use humantime::format_duration;

use super::Config;
use crate::cli::handle_rpc_err;

#[derive(Debug, Subcommand)]
pub enum InfoCommand {
    Show,
}

#[derive(Debug, strum_macros::Display, PartialEq)]
enum SyncStatus {
    Ok,
    Slow,
    Behind,
}

#[derive(Debug)]
pub struct NodeStatusInfo {
    /// duration in seconds of how far behind the node is with respect to
    /// syncing to head
    behind: Duration,
    /// Chain health is the percentage denoting how close we are to having an
    /// average of 5 blocks per tipset in the last couple of hours.
    /// The number of blocks per tipset is non-deterministic but averaging at 5
    /// is considered healthy.
    health: f64,
    /// epoch the node is currently at
    epoch: ChainEpoch,
    /// base fee
    base_fee: TokenAmount,
    /// sync status information
    sync_status: SyncStatus,
    /// Start time of the node
    start_time: DateTime<Utc>,
    /// Current network the node is running on
    network: String,
    /// Default wallet address selected.
    default_wallet_address: Option<String>,
}

impl NodeStatusInfo {
    fn set_default_wallet(&mut self, wallet_addr: Option<String>) {
        self.default_wallet_address = wallet_addr;
    }

    fn set_network(&mut self, network: &str) {
        self.network = network.to_string();
    }

    fn set_node_start_time(&mut self, start_time: DateTime<Utc>) {
        self.start_time = start_time
    }

    fn new(
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

        let health = (100 * block_count) as f64 / (num_tipsets * BLOCKS_PER_EPOCH as usize) as f64;

        Ok(NodeStatusInfo {
            behind: Duration::from_secs(behind),
            health,
            epoch,
            base_fee,
            sync_status,
            start_time: Utc::now(),
            network: String::from("unknown"),
            default_wallet_address: None,
        })
    }

    fn display(&self, color: &LoggingColor) -> NodeInfoOutput {
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
            .unwrap_or("-".to_string())
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

    fn chain_status(&self) -> String {
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

struct NodeInfoOutput {
    chain_status: ColoredString,
    network: ColoredString,
    uptime: ColoredString,
    health: ColoredString,
    wallet_address: ColoredString,
}

impl InfoCommand {
    pub async fn run(&self, config: Config, opts: &CliOpts) -> anyhow::Result<()> {
        // uptime
        let start_time = start_time(&config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;

        let network = chain_get_name((), &config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;

        // Wallet info
        let default_wallet_address = wallet_default_address((), &config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;

        let tipsets = chain_get_tipsets_finality((), &config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;

        let cur_duration_secs = SystemTime::now().duration_since(UNIX_EPOCH)?;

        let mut node_status = NodeStatusInfo::new(
            cur_duration_secs,
            tipsets,
            config.chain.policy.chain_finality as usize,
        )?;
        node_status.set_network(&network);
        node_status.set_node_start_time(start_time);
        node_status.set_default_wallet(default_wallet_address);

        let status_output = node_status.display(&opts.color);

        println!("Network: {}", status_output.network);
        println!("Uptime: {}", status_output.uptime);
        println!("Chain: {}", status_output.chain_status);
        println!("Chain health: {}", status_output.health);
        println!("Default wallet address: {}", status_output.wallet_address);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{str::FromStr, sync::Arc, time::Duration};

    use chrono::{DateTime, Utc};
    use colored::*;
    use forest_blocks::{tipset_json::TipsetJson, BlockHeader, Tipset};
    use forest_cli_shared::logger::LoggingColor;
    use forest_shim::{address::Address, econ::TokenAmount};
    use fvm_shared::clock::EPOCH_DURATION_SECONDS;
    use quickcheck_macros::quickcheck;

    use super::NodeStatusInfo;
    use crate::cli::info_cmd::SyncStatus;

    const CHAIN_FINALITY: usize = 900;

    fn mock_tipset_at(seconds_since_unix_epoch: u64) -> Arc<Tipset> {
        let mock_header = BlockHeader::builder()
            .miner_address(Address::from_str("f2kmbjvz7vagl2z6pfrbjoggrkjofxspp7cqtw2zy").unwrap())
            .timestamp(seconds_since_unix_epoch)
            .build()
            .unwrap();
        let tipset = Tipset::from(&mock_header);

        Arc::new(tipset)
    }

    fn node_status_good() -> NodeStatusInfo {
        super::NodeStatusInfo {
            behind: Duration::from_secs(0),
            health: 90.,
            epoch: i64::MAX,
            base_fee: TokenAmount::from_whole(1),
            sync_status: SyncStatus::Ok,
            start_time: DateTime::<Utc>::MIN_UTC,
            network: "calibnet".to_string(),
            default_wallet_address: Some("-".to_string()),
        }
    }

    fn node_status_bad() -> NodeStatusInfo {
        super::NodeStatusInfo {
            behind: Duration::from_secs(0),
            health: 0.,
            epoch: 0,
            base_fee: TokenAmount::from_whole(1),
            sync_status: SyncStatus::Behind,
            start_time: DateTime::<Utc>::MIN_UTC,
            network: "calibnet".to_string(),
            default_wallet_address: Some("-".to_string()),
        }
    }

    #[quickcheck]
    fn test_sync_status_ok(tipsets: Vec<Arc<Tipset>>) {
        let tipsets = tipsets.iter().map(|ts| TipsetJson(ts.clone())).collect();
        let status_result = NodeStatusInfo::new(Duration::from_secs(0), tipsets, CHAIN_FINALITY);
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

    #[test]
    fn chain_status_test() {
        let cur_duration = Duration::from_secs(100_000);
        let tipset = mock_tipset_at(cur_duration.as_secs() - 59);
        let color = LoggingColor::Never;
        let node_status =
            NodeStatusInfo::new(cur_duration, vec![TipsetJson(tipset)], CHAIN_FINALITY).unwrap();
        let output = node_status.display(&color);
        let expected_status_fmt = "[sync: Slow! (59s behind)] [basefee: 0 atto FIL] [epoch: 0]";
        assert_eq!(expected_status_fmt.clear(), output.chain_status);

        let tipset = mock_tipset_at(cur_duration.as_secs() - 30000);
        let node_status =
            NodeStatusInfo::new(cur_duration, vec![TipsetJson(tipset)], CHAIN_FINALITY).unwrap();
        let output = node_status.display(&color);

        let expected_status_fmt =
            "[sync: Behind! (8h 20m behind)] [basefee: 0 atto FIL] [epoch: 0]";
        assert_eq!(expected_status_fmt.clear(), output.chain_status);
    }

    #[test]
    fn test_node_info_formattting() {
        // no color tests
        let color = LoggingColor::Never;
        let node_status = node_status_bad();
        let info = node_status.display(&color);
        assert_eq!(info.network, "calibnet".normal());
        assert_eq!(info.health, "0.00%\n\n".normal());
        assert_eq!(info.wallet_address, "-".normal());
        assert_eq!(info.chain_status, node_status.chain_status().normal());

        // with color tests
        let color = LoggingColor::Always;
        let node_status = node_status_good();
        let info = node_status.display(&color);
        assert_eq!(info.network, "calibnet".green());
        assert_eq!(info.health, "90.00%\n\n".green());
        assert_eq!(info.wallet_address, "-".bold());
        assert_eq!(info.chain_status, node_status.chain_status().blue());
    }
}
