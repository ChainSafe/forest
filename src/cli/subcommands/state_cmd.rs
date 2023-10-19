// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;

use crate::lotus_json::LotusJson;
use crate::rpc_client::state_ops::state_fetch_root;
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::TokenAmount;
use crate::Client;
use cid::Cid;
use clap::Subcommand;
use serde_tuple::{self, Deserialize_tuple, Serialize_tuple};

use super::handle_rpc_err;

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
}

impl StateCommands {
    pub async fn run(self, client: Client) -> anyhow::Result<()> {
        match self {
            Self::Fetch { root, save_to_file } => {
                println!(
                    "{}",
                    state_fetch_root((LotusJson(root), save_to_file), &client.rpc_token)
                        .await
                        .map_err(handle_rpc_err)?
                );
            }
        }
        Ok(())
    }
}
