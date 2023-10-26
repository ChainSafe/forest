// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::networks::generate_actor_bundle;
use std::path::PathBuf;

#[derive(Debug, clap::Subcommand)]
pub enum StateMigrationCommands {
    /// Generate a merged actor bundle from the hard-coded sources in forest
    ActorBundle {
        #[arg(default_value = "actor_bundles.car.zst")]
        output: PathBuf,
    },
}

impl StateMigrationCommands {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::ActorBundle { output } => {
                generate_actor_bundle(&output).await?;
                println!("Wrote the actors bundle to {}", output.display());
                Ok(())
            }
        }
    }
}
