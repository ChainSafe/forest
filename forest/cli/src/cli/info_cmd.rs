// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::time::Duration;

use chrono::Utc;
use clap::Subcommand;
use colored::*;
use forest_cli_shared::cli::CliOpts;
use forest_rpc_api::data_types::node_api::NodeStatusInfo;
use forest_rpc_client::node_ops::node_status;
use forest_shim::econ::TokenAmount;
use forest_utils::misc::LoggingColor;
use humantime::format_duration;
use num::BigInt;

use super::Config;
use crate::cli::handle_rpc_err;

#[derive(Debug, Subcommand)]
pub enum InfoCommand {
    Show,
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
    wallet_balance: ColoredString,
}

impl From<NodeStatusInfo> for NodeInfoOutput {
    fn from(node_status_info: NodeStatusInfo) -> NodeInfoOutput {
        let health = node_status_info.health;

        // let use_color = color.coloring_enabled();
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

        let wallet_balance = balance(node_status_info.default_wallet_address_balance)
            .unwrap_or("n/a".to_string())
            .bold();

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
    fn display(mut self, color: &LoggingColor) {
        if !color.coloring_enabled() {
            self.chain_status = self.chain_status.clear();
            self.network = self.network.clear();
            self.uptime = self.uptime.clear();
            self.health = self.health.clear();
            self.wallet_address = self.wallet_address.clear();
            self.wallet_balance = self.wallet_balance.clear();
        }

        println!("Network: {}", self.network);
        println!("Uptime: {}", self.uptime);
        println!("Chain: {}", self.chain_status);
        println!("Chain health: {}", self.health);
        println!(
            "Default wallet address: {} [balance: {}]",
            self.wallet_address, self.wallet_balance
        );
    }
}

impl InfoCommand {
    pub async fn run(&self, config: Config, opts: &CliOpts) -> anyhow::Result<()> {
        let node_status = node_status(&config.client.rpc_token)
            .await
            .map_err(handle_rpc_err)?;

        let info_output = NodeInfoOutput::from(node_status);
        info_output.display(&opts.color);

        Ok(())
    }
}

fn balance(balance: Option<String>) -> anyhow::Result<String> {
    use crate::humantoken::TokenAmountPretty;
    if let Some(bal) = balance {
        let balance_token_amount = TokenAmount::from_atto(bal.parse::<BigInt>()?);
        Ok(format!("{:.4}", balance_token_amount.pretty()))
    } else {
        Ok(String::from("n/a"))
    }
}

#[cfg(test)]
mod tests {
    use std::{str::FromStr, sync::Arc, time::Duration};

    use chrono::{DateTime, Utc};
    use colored::*;
    use forest_blocks::{tipset_json::TipsetJson, BlockHeader, Tipset};
    use forest_rpc_api::data_types::node_api::{NodeStatus, NodeStatusInfo, SyncStatus};
    use forest_shim::{address::Address, econ::TokenAmount};
    use forest_utils::misc::LoggingColor;

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
            node_sync_status: NodeStatus::default(),
        }
    }

    // fn node_status_bad() -> NodeStatusInfo {
    //     NodeStatusInfo {
    //         behind: Duration::from_secs(0),
    //         health: 0.,
    //         epoch: 0,
    //         base_fee: TokenAmount::from_whole(1),
    //         sync_status: SyncStatus::Behind,
    //         start_time: DateTime::<Utc>::MIN_UTC,
    //         network: "calibnet".to_string(),
    //         default_wallet_address: Some("-".to_string()),
    //         default_wallet_address_balance: None,
    //         node_sync_status: NodeStatus::default(),
    //     }
    // }

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
        let node_status = mock_node_status();
        let info = node_status.display(&color);
        assert_eq!(info.network, "calibnet".normal());
        assert_eq!(info.health, "90.00%\n\n".normal());
        assert_eq!(info.wallet_address, "-".normal());
        assert_eq!(info.chain_status, node_status.chain_status().normal());

        // with color tests
        let color = LoggingColor::Always;
        let node_status = mock_node_status();
        let info = node_status.display(&color);
        assert_eq!(info.network, "calibnet".green());
        assert_eq!(info.health, "90.00%\n\n".green());
        assert_eq!(info.wallet_address, "-".bold());
        assert_eq!(info.chain_status, node_status.chain_status().blue());
    }
}
