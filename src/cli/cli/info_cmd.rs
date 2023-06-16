// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::blocks::Tipset;
use crate::cli_shared::cli::CliOpts;
use crate::rpc_client::{
    chain_get_name, chain_head, node_ops::node_status, start_time, wallet_balance,
    wallet_default_address,
};
use crate::shim::econ::TokenAmount;
use chrono::{DateTime, Utc};
use clap::Subcommand;

use fvm_shared::clock::EPOCH_DURATION_SECONDS;
use fvm_shared::{clock::ChainEpoch, BLOCKS_PER_EPOCH};
use humantime::format_duration;
use num::BigInt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::Config;
use crate::cli::cli::handle_rpc_err;
use crate::cli::humantoken::TokenAmountPretty;

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
        head: &Tipset,
        start_time: DateTime<Utc>,
        network: String,
        default_wallet_address: Option<String>,
        default_wallet_address_balance: Option<String>,
    ) -> NodeStatusInfo {
        let ts = head.min_timestamp() as i64;
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

        let base_fee = head.min_ticket_block().parent_base_fee().clone();

        // blocks_per_tipset_last_finality = no of blocks till head / chain finality
        let health = 100. * blocks_per_tipset_last_finality / BLOCKS_PER_EPOCH as f64;

        Self {
            lag,
            health,
            epoch: head.epoch(),
            base_fee,
            sync_status,
            start_time,
            network,
            default_wallet_address,
            default_wallet_address_balance,
        }
    }

    fn format(&self, now: DateTime<Utc>) -> String {
        let network = format!("Network: {}", self.network);

        let uptime = {
            let uptime = (now - self.start_time)
                .to_std()
                .expect("failed converting to std duration");
            let uptime = Duration::from_secs(uptime.as_secs());
            let fmt_uptime = format_duration(uptime);
            format!(
                "Uptime: {fmt_uptime} (Started at: {})",
                self.start_time.with_timezone(&chrono::offset::Local)
            )
        };

        let chain = {
            let base_fee_fmt = self.base_fee.pretty();
            let lag_time = humantime::format_duration(Duration::from_secs(self.lag.unsigned_abs()));
            let behind = if self.lag < 0 {
                format!("{} ahead", lag_time)
            } else {
                format!("{} behind", lag_time)
            };

            format!(
                "Chain: [sync: {}! ({})] [basefee: {base_fee_fmt}] [epoch: {}]",
                self.sync_status, behind, self.epoch
            )
        };

        let chain_health = format!("Chain health: {:.2}%\n\n", self.health);

        let wallet_info = {
            let wallet_address = self
                .default_wallet_address
                .clone()
                .unwrap_or("address not set".to_string());

            let wallet_balance = match self
                .default_wallet_address_balance
                .as_ref()
                .map(|s| balance(s))
                .transpose()
            {
                Ok(bal) => format!(
                    "[balance: {}]",
                    bal.unwrap_or("could not find balance".to_string())
                ),
                Err(e) => e.to_string(),
            };

            format!(
                "Default wallet address: {} [{}]",
                wallet_address, wallet_balance
            )
        };

        vec![network, uptime, chain, chain_health, wallet_info].join("\n")
    }
}

impl InfoCommand {
    pub async fn run(&self, config: Config, _opts: &CliOpts) -> anyhow::Result<()> {
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
                    head.0.as_ref(),
                    start_time,
                    network,
                    default_wallet_address.clone(),
                    default_wallet_address_balance,
                );

                println!("{}", node_status_info.format(Utc::now()));

                Ok(())
            }
            Err(e) => Err(handle_rpc_err(e)),
        }
    }
}

fn balance(bal: &str) -> Result<String, anyhow::Error> {
    let balance_token_amount = TokenAmount::from_atto(bal.parse::<BigInt>()?);
    Ok(format!("{:.4}", balance_token_amount.pretty()))
}

#[cfg(test)]
mod tests {
    use crate::blocks::{BlockHeader, Tipset};
    use crate::shim::{address::Address, econ::TokenAmount};
    use chrono::DateTime;
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

    fn node_status(duration: Duration, tipset: &Tipset) -> NodeStatusInfo {
        NodeStatusInfo::new(
            duration,
            20.,
            tipset,
            DateTime::<chrono::Utc>::MIN_UTC,
            "calibnet".to_string(),
            None,
            None,
        )
    }

    #[quickcheck]
    fn test_sync_status_ok(duration: Duration) {
        let tipset = mock_tipset_at(duration.as_secs() + (EPOCH_DURATION_SECONDS as u64 * 3 / 2));

        let status = node_status(duration, tipset.as_ref());

        assert_ne!(status.sync_status, SyncStatus::Slow);
        assert_ne!(status.sync_status, SyncStatus::Behind);
    }

    #[quickcheck]
    fn test_sync_status_behind(duration: Duration) {
        let duration = duration + Duration::from_secs(300);
        let tipset = mock_tipset_at(duration.as_secs().saturating_sub(200));
        let status = node_status(duration, tipset.as_ref());

        assert!(status.health.is_finite());
        assert_ne!(status.sync_status, SyncStatus::Ok);
        assert_ne!(status.sync_status, SyncStatus::Slow);
    }

    #[quickcheck]
    fn test_sync_status_slow(duration: Duration) {
        let duration = duration + Duration::from_secs(300);
        let tipset = mock_tipset_at(
            duration
                .as_secs()
                .saturating_sub(EPOCH_DURATION_SECONDS as u64 * 4),
        );
        let status = node_status(duration, tipset.as_ref());
        assert!(status.health.is_finite());
        assert_ne!(status.sync_status, SyncStatus::Behind);
        assert_ne!(status.sync_status, SyncStatus::Ok);
    }

    #[test]
    fn block_sync_timestamp() {
        let duration = Duration::from_secs(60);
        let tipset = mock_tipset_at(duration.as_secs() - 10);
        let status = node_status(duration, tipset.as_ref());

        assert!(status
            .format(DateTime::<chrono::Utc>::MIN_UTC)
            .contains("10s behind"));
    }

    #[test]
    fn test_lag_uptime_ahead() {
        let mut status = mock_node_status();
        status.lag = -360;
        assert!(status
            .format(DateTime::<chrono::Utc>::MIN_UTC)
            .contains("6m ahead"));
    }

    #[test]
    fn chain_status_test() {
        let duration = Duration::from_secs(100_000);
        let tipset = mock_tipset_at(duration.as_secs() - 59);
        let status = node_status(duration, tipset.as_ref());
        let expected_status_fmt =
            "[sync: Slow! (59s behind)] [basefee: 0 FIL] [epoch: 0]".to_string();
        assert!(status
            .format(DateTime::<chrono::Utc>::MIN_UTC)
            .contains(&expected_status_fmt));

        let tipset = mock_tipset_at(duration.as_secs() - 30000);
        let status = node_status(duration, tipset.as_ref());

        let expected_status_fmt =
            "[sync: Behind! (8h 20m behind)] [basefee: 0 FIL] [epoch: 0]".to_string();
        assert!(status
            .format(DateTime::<chrono::Utc>::MIN_UTC)
            .contains(&expected_status_fmt));
    }
}
