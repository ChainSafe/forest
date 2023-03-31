// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use clap::Subcommand;
use forest_json::cid::vec::CidJsonVec;
use forest_rpc_client::mpool_ops::*;
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
            Self::Pending { local, cids } => {
                let response = mpool_pending((CidJsonVec(vec![]),), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;
                for msg in response {
                    if cids {
                        println!("{}", msg.0.cid()?);
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
