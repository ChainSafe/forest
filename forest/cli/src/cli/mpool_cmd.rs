// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use ahash::HashSet;
use clap::Subcommand;
use forest_json::cid::vec::CidJsonVec;
use forest_rpc_client::{mpool_ops::mpool_pending, wallet_ops::wallet_list};
use forest_shim::address::Address;
use fvm_ipld_encoding::Cbor;

use super::Config;
use crate::cli::handle_rpc_err;

#[derive(Debug, Subcommand)]
pub enum MpoolCommands {
    /// Get pending messages
    Pending {
        /// Print pending messages for addresses in local wallet only
        #[arg(long)]
        local: bool,
        /// Only print cids of messages in output
        #[arg(long)]
        cids: bool,
        /// Return messages to a given address
        #[arg(long)]
        to: Option<Address>,
        /// Return messages from a given addres
        #[arg(long)]
        from: Option<Address>,
    },
    /// Print mempool stats
    Stat {
        /// Number of blocks to look back for minimum base fee
        #[arg(short, default_value = "60")]
        base_fee_lookback: u32,
        /// Print stats for local addresses only
        #[arg(short)]
        local: bool,
    },
}

impl MpoolCommands {
    pub async fn run(&self, config: Config) -> anyhow::Result<()> {
        match *self {
            Self::Pending {
                local,
                cids,
                to,
                from,
            } => {
                let messages = mpool_pending((CidJsonVec(vec![]),), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;

                let wallet_addrs = if local {
                    let response = wallet_list((), &config.client.rpc_token)
                        .await
                        .map_err(handle_rpc_err)?;
                    Some(HashSet::from_iter(
                        response.iter().filter_map(|addr| addr.0.cid().ok()),
                    ))
                } else {
                    None
                };

                let filtered_messages = messages.iter().filter(|msg| {
                    wallet_addrs
                        .as_ref()
                        .map(|addrs| !addrs.contains(&msg.0.cid().unwrap()))
                        .unwrap_or(true)
                        && to.map(|addr| msg.0.message().to == *addr).unwrap_or(true)
                        && from
                            .map(|addr| msg.0.message().from == *addr)
                            .unwrap_or(true)
                });
                for msg in filtered_messages {
                    if cids {
                        println!("{}", msg.0.cid().unwrap());
                    } else {
                        println!("{}", serde_json::to_string_pretty(&msg)?);
                    }
                }
                Ok(())
            }
            Self::Stat {
                base_fee_lookback,
                local,
            } => {
                // TODO: revive stat code
                Ok(())
            }
        }
    }
}
