// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::json::cid::CidJson;
use crate::rpc_client::state_ops::state_fetch_root;
use crate::shim::clock::ChainEpoch;
use cid::Cid;
use clap::Subcommand;
use fvm_shared::econ::TokenAmount;
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
    Fetch { root: Cid },
}

impl StateCommands {
    pub async fn run(self, config: Config) -> anyhow::Result<()> {
        match self {
            Self::Fetch { root } => {
                println!(
                    "{}",
                    state_fetch_root((CidJson(root),), &config.client.rpc_token)
                        .await
                        .map_err(handle_rpc_err)?
                );
            }
        }
        Ok(())
    }
}
