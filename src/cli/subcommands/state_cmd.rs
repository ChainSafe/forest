// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::path::PathBuf;
use std::time::Duration;

use crate::rpc::state::StateCompute;
use crate::rpc::{self, prelude::*};
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::TokenAmount;
use cid::Cid;
use clap::Subcommand;
use serde_tuple::{self, Deserialize_tuple, Serialize_tuple};

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
    Compute {
        /// Which epoch to compute the state transition for
        #[arg(long)]
        epoch: ChainEpoch,
    },
}

impl StateCommands {
    pub async fn run(self, client: rpc::Client) -> anyhow::Result<()> {
        match self {
            Self::Fetch { root, save_to_file } => {
                let ret = client
                    .call(
                        StateFetchRoot::request((root, save_to_file))?.with_timeout(Duration::MAX),
                    )
                    .await?;
                println!("{ret}");
            }
            StateCommands::Compute { epoch } => {
                let ret = client
                    .call(StateCompute::request((epoch,))?.with_timeout(Duration::MAX))
                    .await?;
                println!("{ret}");
            }
        }
        Ok(())
    }
}
