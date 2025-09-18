// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::lotus_json::HasLotusJson;
use crate::rpc::state::{ForestComputeStateOutput, ForestStateCompute};
use crate::rpc::{self, prelude::*};
use crate::shim::address::StrictAddress;
use crate::shim::clock::ChainEpoch;
use cid::Cid;
use clap::Subcommand;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum Format {
    Json,
    Text,
}

#[derive(Debug, Subcommand)]
pub enum StateCommands {
    Fetch {
        root: Cid,
        /// The `.car` file path to save the state root
        #[arg(short, long)]
        save_to_file: Option<PathBuf>,
    },
    /// Compute state trees for epochs
    Compute {
        /// Which epoch to compute the state transition for
        #[arg(long)]
        epoch: ChainEpoch,
        /// Number of tipset epochs to compute state for. Default is 1
        #[arg(short, long)]
        n_epochs: Option<NonZeroUsize>,
        /// Print epoch and tipset key along with state root
        #[arg(short, long)]
        verbose: bool,
    },
    /// Read the state of an actor
    ReadState {
        /// Actor address to read the state of
        actor_address: StrictAddress,
    },
    /// Returns the built-in actor bundle CIDs for the current network
    ActorCids {
        /// Format output
        #[arg(long, default_value = "text")]
        format: Format,
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
            StateCommands::Compute {
                epoch,
                n_epochs,
                verbose,
            } => {
                let results = client
                    .call(
                        ForestStateCompute::request((epoch, n_epochs))?.with_timeout(Duration::MAX),
                    )
                    .await?;
                for ForestComputeStateOutput {
                    state_root,
                    epoch,
                    tipset_key,
                } in results
                {
                    if verbose {
                        println!("{state_root} (epoch: {epoch}, tipset key: {tipset_key})");
                    } else {
                        println!("{state_root}");
                    }
                }
            }
            Self::ReadState { actor_address } => {
                let tipset = ChainHead::call(&client, ()).await?;
                let ret = client
                    .call(
                        StateReadState::request((actor_address.into(), tipset.key().into()))?
                            .with_timeout(Duration::MAX),
                    )
                    .await?;
                println!("{}", ret.state.into_lotus_json_string_pretty()?);
            }
            Self::ActorCids { format } => {
                let info = client.call(StateActorInfo::request(())?).await?;

                match format {
                    Format::Json => {
                        println!("{}", serde_json::to_string_pretty(&info)?);
                    }
                    Format::Text => println!("{info}"),
                }
            }
        }
        Ok(())
    }
}
