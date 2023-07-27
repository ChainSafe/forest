// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use crate::db::db_engine::db_root;
use crate::db::db_engine::open_proxy_db;
use crate::json::cid::CidJson;
use crate::rpc_client::state_ops::state_fetch_root;
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::TokenAmount;
use crate::statediff::print_state_diff;
use cid::Cid;
use clap::Subcommand;
use serde_tuple::{self, Deserialize_tuple, Serialize_tuple};

use super::handle_rpc_err;
use super::Config;

#[derive(Serialize_tuple, Deserialize_tuple, Clone, Debug)]
struct VestingSchedule {
    entries: Vec<VestingScheduleEntry>,
}

#[derive(Serialize_tuple, Deserialize_tuple, Clone, Debug)]
struct VestingScheduleEntry {
    epoch: ChainEpoch,
    amount: TokenAmount,
}

#[derive(Debug, Subcommand)]
pub enum StateCommands {
    Fetch {
        root: Cid,
        /// The `.car` file path to save the state root
        #[arg(short, long)]
        save_to_file: Option<PathBuf>,
    },
    Diff {
        /// The previous CID state root
        pre: Cid,
        /// The post CID state root
        post: Cid,
        /// The depth at which IPLD links are resolved
        #[arg(short, long)]
        depth: Option<u64>,
    },
}

impl StateCommands {
    pub async fn run(self, config: Config) -> anyhow::Result<()> {
        match self {
            Self::Fetch { root, save_to_file } => {
                println!(
                    "{}",
                    state_fetch_root((CidJson(root), save_to_file), &config.client.rpc_token)
                        .await
                        .map_err(handle_rpc_err)?
                );
            }
            Self::Diff { pre, post, depth } => {
                let chain_path = config
                    .client
                    .data_dir
                    .join(config.chain.network.to_string());
                let blockstore = open_proxy_db(db_root(&chain_path), Default::default())?;

                if let Err(err) = print_state_diff(&blockstore, &pre, &post, depth) {
                    eprintln!("Failed to print state diff: {err}");
                }
            }
        }
        Ok(())
    }
}
