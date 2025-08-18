// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::lotus_json::HasLotusJson;
use crate::rpc::state::ForestStateCompute;
use crate::rpc::{self, prelude::*};
use crate::shim::address::{CurrentNetwork, Error, Network, StrictAddress};
use crate::shim::clock::ChainEpoch;
use cid::Cid;
use clap::Subcommand;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::Duration;

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
        /// Number of tipset epochs to compute state for. Default is 1
        #[arg(short, long)]
        n_epochs: Option<NonZeroUsize>,
    },
    /// Read the state of an actor
    ReadState {
        /// Actor address to read the state of
        actor_address: String,
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
            StateCommands::Compute { epoch, n_epochs } => {
                let state_roots = client
                    .call(
                        ForestStateCompute::request((epoch, n_epochs))?.with_timeout(Duration::MAX),
                    )
                    .await?;
                for state_root in state_roots {
                    println!("{state_root}");
                }
            }
            Self::ReadState { actor_address } => {
                let tipset = ChainHead::call(&client, ()).await?;
                let address = match StrictAddress::from_str(&actor_address) {
                    Ok(address) => address.into(),
                    Err(Error::UnknownNetwork) => {
                        let expected = match CurrentNetwork::get() {
                            Network::Mainnet => 'f',
                            Network::Testnet => 't',
                        };
                        anyhow::bail!("Invalid network prefix, expected '{}'", expected);
                    }
                    Err(e) => anyhow::bail!("Error parsing address: {e}"),
                };

                let ret = client
                    .call(
                        StateReadState::request((address, tipset.key().into()))?
                            .with_timeout(Duration::MAX),
                    )
                    .await?;
                println!("{}", ret.state.into_lotus_json_string_pretty()?);
            }
        }
        Ok(())
    }
}
