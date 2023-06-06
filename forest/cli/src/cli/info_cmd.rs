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

use fvm_shared::clock::EPOCH_DURATION_SECONDS;
use fvm_shared::{clock::ChainEpoch, BLOCKS_PER_EPOCH};
use humantime::format_duration;
use num::BigInt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::Config;
use crate::cli::handle_rpc_err;
use crate::humantoken::TokenAmountPretty;

#[derive(Debug, Subcommand)]
pub enum InfoCommand {
    Show,
}

#[derive(Debug)]
pub struct NodeStatusInfo {
    /// How far behind the node is with respect to syncing to head
    pub lag: Duration,
    /// Chain health is the percentage denoting how close we are to having
    /// an average of 5 blocks per tipset in the last couple of
    /// hours. The number of blocks per tipset is non-deterministic
    /// but averaging at 5 is considered healthy.
    pub health: f64,
    /// epoch the node is currently at
    pub epoch: ChainEpoch,
    /// Base fee is the set price per unit of gas (measured in attoFIL/gas unit) to be burned (sent to an unrecoverable address) for every message execution
    pub base_fee: TokenAmount,
    pub sync_status: SyncStatus,
    /// Start time of the node
    pub start_time: Option<DateTime<Utc>>,
    pub network: Option<String>,
    pub default_wallet_address: Option<String>,
    pub default_wallet_address_balance: Option<String>,
    use_color: bool,
}

impl NodeStatusInfo {
    fn chain_status(&self) -> ColoredString {
        let base_fee_fmt = self.base_fee.pretty();
        let behind = format!("{}", humantime::format_duration(self.lag));
        let chain_status = format!(
            "[sync: {}! ({} behind)] [basefee: {base_fee_fmt}] [epoch: {}]",
            self.sync_status, behind, self.epoch
        )
        .blue();

        if !self.use_color {
            chain_status.clear()
        } else {
            chain_status
        }
    }

    fn network(&self) -> ColoredString {
        let network = if let Some(network) = &self.network {
            network.green()
        } else {
            "-".green()
        };

        if !self.use_color {
            network.clear()
        } else {
            network
        }
    }

    fn wallet_address(&self) -> ColoredString {
        let wallet_address = self
            .default_wallet_address
            .clone()
            .unwrap_or("address not set".to_string())
            .bold();

        if !self.use_color {
            wallet_address.clear()
        } else {
            wallet_address
        }
    }

    fn uptime(&self, now: DateTime<Utc>) -> ColoredString {
        let uptime = if let Some(start_time) = self.start_time {
            let uptime = (now - start_time)
                .to_std()
                .expect("failed converting to std duration");
            let uptime = Duration::from_secs(uptime.as_secs());
            let fmt_uptime = format_duration(uptime);
            format!(
                "{fmt_uptime} (Started at: {})",
                start_time.with_timezone(&chrono::offset::Local)
            )
            .normal()
        } else {
            "-".normal()
        };

        if !self.use_color {
            uptime.clear()
        } else {
            uptime
        }
    }

    fn health(&self) -> ColoredString {
        let health = {
            let health = self.health;
            let h = format!("{health:.2}%\n\n");
            if self.health > 85. {
                h.green()
            } else if self.health > 50. {
                h.yellow()
            } else {
                h.red()
            }
        };

        if !self.use_color {
            health.clear()
        } else {
            health
        }
    }

    fn wallet_balance(&self) -> ColoredString {
        let wallet_balance = match balance(&self.default_wallet_address_balance) {
            Ok(bal) => format!("[balance: {}]", bal).bold(),
            Err(_) => "".bold(),
        };

        if !self.use_color {
            wallet_balance.clear()
        } else {
            wallet_balance
        }
    }
}

#[derive(Debug, strum::Display, PartialEq)]
pub enum SyncStatus {
    Ok,
    Slow,
    Behind,
}

impl NodeStatusInfo {
    pub fn new(
        cur_duration: Duration,
        blocks_per_tipset_last_finality: f64,
        head: TipsetJson,
        use_color: bool,
    ) -> anyhow::Result<NodeStatusInfo> {
        let ts = head.0.min_timestamp();
        let cur_duration_secs = cur_duration.as_secs();
        let lag = if ts <= cur_duration_secs + 1 {
            cur_duration_secs.saturating_sub(ts)
        } else {
            anyhow::bail!(
                "System time should not be behind tipset timestamp, please sync the system clock."
            );
        };

        let sync_status = if lag < EPOCH_DURATION_SECONDS as u64 * 3 / 2 {
            // within 1.5 epochs
            SyncStatus::Ok
        } else if lag < EPOCH_DURATION_SECONDS as u64 * 5 {
            // within 5 epochs
            SyncStatus::Slow
        } else {
            SyncStatus::Behind
        };

        let base_fee = head.0.min_ticket_block().parent_base_fee().clone();

        // blocks_per_tipset_last_finality = no of blocks till head / chain finality
        let health = 100. * blocks_per_tipset_last_finality / BLOCKS_PER_EPOCH as f64;

        Ok(Self {
            lag: Duration::from_secs(lag),
            health,
            epoch: head.0.epoch(),
            base_fee,
            sync_status,
            start_time: None,
            network: None,
            use_color,
            default_wallet_address: None,
            default_wallet_address_balance: None,
        })
    }

    fn display(&mut self) {
        println!("Network: {}", self.network());
        println!("Uptime: {}", self.uptime(Utc::now()));
        println!("Chain: {}", self.chain_status());
        println!("Chain health: {}", self.health());
        println!(
            "Default wallet address: {} {}",
            self.wallet_address(),
            self.wallet_balance()
        );
    }
}

impl InfoCommand {
    pub async fn run(&self, config: Config, opts: &CliOpts) -> anyhow::Result<()> {
        let res = tokio::try_join!(
            node_status((), &config.client.rpc_token),
            chain_head(&config.client.rpc_token),
            chain_get_name((), &config.client.rpc_token),
            start_time(&config.client.rpc_token)
        );
        match res {
            Ok((node_status, head, network, start_time)) => {
                let cur_duration: Duration = SystemTime::now().duration_since(UNIX_EPOCH)?;
                let blocks_per_tipset_last_finality =
                    node_status.chain_status.blocks_per_tipset_last_finality;

                let mut node_status_info = NodeStatusInfo::new(
                    cur_duration,
                    blocks_per_tipset_last_finality,
                    head,
                    opts.color.coloring_enabled(),
                )?;

                let default_wallet_address = wallet_default_address((), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;

                let default_wallet_address_balance = if let Some(def_addr) = &default_wallet_address
                {
                    let balance = wallet_balance((def_addr.clone(),), &config.client.rpc_token)
                        .await
                        .map_err(handle_rpc_err)?;
                    Some(balance)
                } else {
                    None
                };

                node_status_info.start_time = Some(start_time);
                node_status_info.network = Some(network);
                node_status_info.default_wallet_address = default_wallet_address.clone();
                node_status_info.default_wallet_address_balance = default_wallet_address_balance;

                node_status_info.display();

                Ok(())
            }
            Err(e) => Err(handle_rpc_err(e)),
        }
    }
}

fn balance(balance: &Option<String>) -> anyhow::Result<String> {
    if let Some(bal) = balance {
        let balance_token_amount = TokenAmount::from_atto(bal.parse::<BigInt>()?);
        Ok(format!("{:.4}", balance_token_amount.pretty()))
    } else {
        Err(anyhow::anyhow!("error parsing balance"))
    }
}

#[cfg(test)]
mod tests {
    use colored::*;
    use forest_blocks::{tipset_json::TipsetJson, BlockHeader, Tipset};
    use forest_shim::address::Address;
    use fvm_shared::clock::EPOCH_DURATION_SECONDS;
    use quickcheck_macros::quickcheck;
    use std::{str::FromStr, sync::Arc, time::Duration};

    use super::{NodeStatusInfo, SyncStatus};

    fn mock_tipset_at(seconds_since_unix_epoch: u64) -> Arc<Tipset> {
        let mock_header = BlockHeader::builder()
            .miner_address(Address::from_str("f2kmbjvz7vagl2z6pfrbjoggrkjofxspp7cqtw2zy").unwrap())
            .timestamp(seconds_since_unix_epoch)
            .build()
            .unwrap();
        let tipset = Tipset::from(&mock_header);

        Arc::new(tipset)
    }

    // fn mock_node_status() -> NodeStatusInfo {
    //     NodeStatusInfo {
    //         lag: Duration::from_secs(0),
    //         health: 90.,
    //         epoch: i64::MAX,
    //         base_fee: TokenAmount::from_whole(1),
    //         sync_status: SyncStatus::Ok,
    //         start_time: Some(DateTime::<Utc>::MIN_UTC),
    //         network: Some("calibnet".to_string()),
    //         default_wallet_address: Some("-".to_string()),
    //         default_wallet_address_balance: None,
    //     }
    // }

    #[quickcheck]
    fn test_sync_status_ok(duration: Duration) {
        let status_result = NodeStatusInfo::new(
            duration,
            20.,
            TipsetJson(mock_tipset_at(
                duration.as_secs() + (EPOCH_DURATION_SECONDS as u64 * 3 / 2),
            )),
            false,
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
        let node_status = NodeStatusInfo::new(duration, 20., TipsetJson(tipset), false).unwrap();
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
        let node_status = NodeStatusInfo::new(duration, 20., TipsetJson(tipset), false).unwrap();
        assert!(node_status.health.is_finite());
        assert_ne!(node_status.sync_status, SyncStatus::Behind);
        assert_ne!(node_status.sync_status, SyncStatus::Ok);
    }

    #[test]
    fn block_sync_timestamp() {
        let duration = Duration::from_secs(60);
        let tipset = mock_tipset_at(duration.as_secs() - 10);
        let node_status = NodeStatusInfo::new(duration, 20., TipsetJson(tipset), false).unwrap();
        assert!(node_status.chain_status().contains("10s behind"));
    }

    #[test]
    fn chain_status_test() {
        let cur_duration = Duration::from_secs(100_000);
        let tipset = mock_tipset_at(cur_duration.as_secs() - 59);
        let node_status =
            NodeStatusInfo::new(cur_duration, 20., TipsetJson(tipset), false).unwrap();

        let expected_status_fmt = "[sync: Slow! (59s behind)] [basefee: 0 FIL] [epoch: 0]";
        assert_eq!(expected_status_fmt.clear(), node_status.chain_status());

        let tipset = mock_tipset_at(cur_duration.as_secs() - 30000);
        let node_status =
            NodeStatusInfo::new(cur_duration, 20., TipsetJson(tipset), false).unwrap();

        let expected_status_fmt = "[sync: Behind! (8h 20m behind)] [basefee: 0 FIL] [epoch: 0]";
        assert_eq!(expected_status_fmt.clear(), node_status.chain_status());
    }

    // #[test]
    // fn test_node_info_formattting() {
    // no color tests
    // let color = LoggingColor::Never;
    // let node_status = mock_node_status();
    // let chain_status = node_status.chain_status();
    // let info = NodeInfoOutput::from(node_status).set_color(&color);
    // assert_eq!(node_status.network(), "calibnet".normal());
    // assert_eq!(node_status.health(), "90.00%\n\n".normal());
    // assert_eq!(node_status.wallet_address(), "-".normal());
    // assert_eq!(node_status.chain_status(), chain_status.normal());

    // with color tests
    // let color = LoggingColor::Always;
    // let node_status = mock_node_status();
    // let chain_status = node_status.chain_status();
    // let info = NodeInfoOutput::from(node_status).set_color(&color);
    // assert_eq!(node_status.network, "calibnet".green());
    // assert_eq!(node_status.health, "90.00%\n\n".green());
    // assert_eq!(node_status.wallet_address, "-".bold());
    // assert_eq!(node_status.chain_status(), chain_status.blue());
    // }
}
