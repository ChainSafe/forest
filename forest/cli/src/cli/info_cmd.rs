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
    /// How far behind the node is with respect to syncing to head in seconds
    pub lag: i64,
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
    pub start_time: DateTime<Utc>,
    pub network: String,
    pub default_wallet_address: Option<String>,
    pub default_wallet_address_balance: Option<String>,
}

impl NodeStatusInfo {
    fn chain_status(&self) -> ColoredString {
        let base_fee_fmt = self.base_fee.pretty();
        let lag_time = humantime::format_duration(Duration::from_secs(self.lag.unsigned_abs()));
        let behind = if self.lag < 0 {
            format!("{} ahead", lag_time)
        } else {
            format!("{} behind", lag_time)
        };

        format!(
            "[sync: {}! ({})] [basefee: {base_fee_fmt}] [epoch: {}]",
            self.sync_status, behind, self.epoch
        )
        .blue()
    }

    fn network(&self) -> ColoredString {
        self.network.green()
    }

    fn wallet_address(&self) -> ColoredString {
        self.default_wallet_address
            .clone()
            .unwrap_or("address not set".to_string())
            .bold()
    }

    fn uptime(&self, now: DateTime<Utc>) -> ColoredString {
        let uptime = (now - self.start_time)
            .to_std()
            .expect("failed converting to std duration");
        let uptime = Duration::from_secs(uptime.as_secs());
        let fmt_uptime = format_duration(uptime);
        format!(
            "{fmt_uptime} (Started at: {})",
            self.start_time.with_timezone(&chrono::offset::Local)
        )
        .normal()
    }

    fn health(&self) -> ColoredString {
        let health = self.health;
        let h = format!("{health:.2}%\n\n");
        if self.health > 85. {
            h.green()
        } else if self.health > 50. {
            h.yellow()
        } else {
            h.red()
        }
    }

    fn wallet_balance(&self) -> ColoredString {
        match balance(&self.default_wallet_address_balance) {
            Ok(bal) => format!("[balance: {}]", bal).bold(),
            Err(_) => "".bold(),
        }
    }
}

#[derive(Debug, strum::Display, PartialEq)]
pub enum SyncStatus {
    Ok,
    Slow,
    Behind,
    Fast,
}

impl NodeStatusInfo {
    pub fn new(
        cur_duration: Duration,
        blocks_per_tipset_last_finality: f64,
        head: TipsetJson,
        start_time: DateTime<Utc>,
        network: String,
        default_wallet_address: Option<String>,
        default_wallet_address_balance: Option<String>,
    ) -> NodeStatusInfo {
        let ts = head.0.min_timestamp() as i64;
        let cur_duration_secs = cur_duration.as_secs() as i64;
        let lag = cur_duration_secs - ts;

        let sync_status = if lag < 0 {
            SyncStatus::Fast
        } else if lag < EPOCH_DURATION_SECONDS * 3 / 2 {
            // within 1.5 epochs
            SyncStatus::Ok
        } else if lag < EPOCH_DURATION_SECONDS * 5 {
            // within 5 epochs
            SyncStatus::Slow
        } else {
            SyncStatus::Behind
        };

        let base_fee = head.0.min_ticket_block().parent_base_fee().clone();

        // blocks_per_tipset_last_finality = no of blocks till head / chain finality
        let health = 100. * blocks_per_tipset_last_finality / BLOCKS_PER_EPOCH as f64;

        Self {
            lag,
            health,
            epoch: head.0.epoch(),
            base_fee,
            sync_status,
            start_time,
            network,
            default_wallet_address,
            default_wallet_address_balance,
        }
    }

    fn format(&self, now: DateTime<Utc>, use_color: bool) -> String {
        let lines: Vec<ColoredString> = vec![
            self.network(),
            self.uptime(now),
            self.chain_status(),
            self.health(),
            self.wallet_address(),
            self.wallet_balance(),
        ]
        .into_iter()
        .map(|cs| if use_color { cs } else { cs.clear() })
        .collect();

        use std::fmt::Write;

        let mut output = String::new();
        writeln!(&mut output, "Network: {}\n", lines[0]).unwrap();
        writeln!(&mut output, "Uptime: {}", lines[1]).unwrap();
        writeln!(&mut output, "Chain: {}", lines[2]).unwrap();
        writeln!(&mut output, "Chain health: {}", lines[3]).unwrap();
        writeln!(
            &mut output,
            "Default wallet address: {} {}",
            lines[4], lines[5]
        )
        .unwrap();

        output
    }
}

impl InfoCommand {
    pub async fn run(&self, config: Config, opts: &CliOpts) -> anyhow::Result<()> {
        let res = tokio::try_join!(
            node_status((), &config.client.rpc_token),
            chain_head(&config.client.rpc_token),
            chain_get_name((), &config.client.rpc_token),
            start_time(&config.client.rpc_token),
            wallet_default_address((), &config.client.rpc_token)
        );

        match res {
            Ok((node_status, head, network, start_time, default_wallet_address)) => {
                let cur_duration: Duration = SystemTime::now().duration_since(UNIX_EPOCH)?;
                let blocks_per_tipset_last_finality =
                    node_status.chain_status.blocks_per_tipset_last_finality;

                let default_wallet_address_balance = if let Some(def_addr) = &default_wallet_address
                {
                    let balance = wallet_balance((def_addr.clone(),), &config.client.rpc_token)
                        .await
                        .map_err(handle_rpc_err)?;
                    Some(balance)
                } else {
                    None
                };

                let node_status_info = NodeStatusInfo::new(
                    cur_duration,
                    blocks_per_tipset_last_finality,
                    head,
                    start_time,
                    network,
                    default_wallet_address.clone(),
                    default_wallet_address_balance,
                );

                print!(
                    "{}",
                    node_status_info.format(Utc::now(), opts.color.coloring_enabled())
                );

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
        Err(anyhow::anyhow!("could not find balance"))
    }
}

#[cfg(test)]
mod tests {
    use chrono::DateTime;
    use colored::*;
    use forest_blocks::{tipset_json::TipsetJson, BlockHeader, Tipset};
    use forest_shim::{address::Address, econ::TokenAmount};
    use forest_utils::misc::LoggingColor;
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

    fn mock_node_status() -> NodeStatusInfo {
        NodeStatusInfo {
            lag: 0,
            health: 90.,
            epoch: i64::MAX,
            base_fee: TokenAmount::from_whole(1),
            sync_status: SyncStatus::Ok,
            start_time: DateTime::<chrono::Utc>::MIN_UTC,
            network: "calibnet".to_string(),
            default_wallet_address: Some("-".to_string()),
            default_wallet_address_balance: None,
        }
    }

    #[quickcheck]
    fn test_sync_status_ok(duration: Duration) {
        let status = NodeStatusInfo::new(
            duration,
            20.,
            TipsetJson(mock_tipset_at(
                duration.as_secs() + (EPOCH_DURATION_SECONDS as u64 * 3 / 2),
            )),
            DateTime::<chrono::Utc>::MIN_UTC,
            "calibnet".to_string(),
            None,
            None,
        );

        assert_ne!(status.sync_status, SyncStatus::Slow);
        assert_ne!(status.sync_status, SyncStatus::Behind);
    }

    #[quickcheck]
    fn test_sync_status_behind(duration: Duration) {
        let duration = duration + Duration::from_secs(300);
        let tipset = mock_tipset_at(duration.as_secs().saturating_sub(200));
        let node_status = NodeStatusInfo::new(
            duration,
            20.,
            TipsetJson(tipset),
            DateTime::<chrono::Utc>::MIN_UTC,
            "calibnet".to_string(),
            None,
            None,
        );
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
        let node_status = NodeStatusInfo::new(
            duration,
            20.,
            TipsetJson(tipset),
            DateTime::<chrono::Utc>::MIN_UTC,
            "calibnet".to_string(),
            None,
            None,
        );
        assert!(node_status.health.is_finite());
        assert_ne!(node_status.sync_status, SyncStatus::Behind);
        assert_ne!(node_status.sync_status, SyncStatus::Ok);
    }

    #[test]
    fn block_sync_timestamp() {
        let duration = Duration::from_secs(60);
        let tipset = mock_tipset_at(duration.as_secs() - 10);
        let node_status = NodeStatusInfo::new(
            duration,
            20.,
            TipsetJson(tipset),
            DateTime::<chrono::Utc>::MIN_UTC,
            "calibnet".to_string(),
            None,
            None,
        );
        assert!(node_status.chain_status().contains("10s behind"));
    }

    #[test]
    fn test_lag_uptime_ahead() {
        let mut node_status = mock_node_status();
        node_status.lag = -360;
        assert!(node_status.chain_status().contains("6m ahead"));
    }

    #[test]
    fn chain_status_test() {
        let cur_duration = Duration::from_secs(100_000);
        let tipset = mock_tipset_at(cur_duration.as_secs() - 59);
        let node_status = NodeStatusInfo::new(
            cur_duration,
            20.,
            TipsetJson(tipset),
            DateTime::<chrono::Utc>::MIN_UTC,
            "calibnet".to_string(),
            None,
            None,
        );

        let expected_status_fmt = "[sync: Slow! (59s behind)] [basefee: 0 FIL] [epoch: 0]";
        assert_eq!(expected_status_fmt.blue(), node_status.chain_status());

        let tipset = mock_tipset_at(cur_duration.as_secs() - 30000);
        let node_status = NodeStatusInfo::new(
            cur_duration,
            20.,
            TipsetJson(tipset),
            DateTime::<chrono::Utc>::MIN_UTC,
            "calibnet".to_string(),
            None,
            None,
        );

        let expected_status_fmt = "[sync: Behind! (8h 20m behind)] [basefee: 0 FIL] [epoch: 0]";
        assert_eq!(expected_status_fmt.blue(), node_status.chain_status());
    }

    #[test]

    fn test_node_info_formattting() {
        // no color tests
        #[rustfmt::skip]
        let no_color_expected_output = r#"Network: calibnet
Uptime: 524277years 2months 24days 20h 52m 47s (Started at: -262144-01-01 00:00:00 +00:00)
Chain: [sync: Ok! (0s behind)] [basefee: 1 FIL] [epoch: 9223372036854775807]
Chain health: 90.00%


Default wallet address: - 
"#;

        let node_status = mock_node_status();
        assert_eq!(
            node_status.format(DateTime::<chrono::Utc>::MAX_UTC, false),
            no_color_expected_output
        );

        let color = LoggingColor::default();
        if color.coloring_enabled() {
            // with color tests
            #[rustfmt::skip]
            let with_color_expected_output = "Network: \u{1b}[32mcalibnet\u{1b}[0m
Uptime: 524277years 2months 24days 20h 52m 47s (Started at: -262144-01-01 00:00:00 +00:00)
Chain: \u{1b}[34m[sync: Ok! (0s behind)] [basefee: 1 FIL] [epoch: 9223372036854775807]\u{1b}[0m
Chain health: \u{1b}[32m90.00%\n\n\u{1b}[0m
Default wallet address: \u{1b}[1m-\u{1b}[0m \u{1b}[1m\u{1b}[0m
";
            let node_status = mock_node_status();
            assert_eq!(
                node_status.format(DateTime::<chrono::Utc>::MAX_UTC, color.coloring_enabled()),
                with_color_expected_output
            );
        }
    }
}
