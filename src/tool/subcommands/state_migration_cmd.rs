// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::networks::{generate_actor_bundle, get_actor_bundles_metadata};
use std::path::PathBuf;

#[derive(Debug, clap::Subcommand)]
pub enum StateMigrationCommands {
    /// Generate a merged actor bundle from the hard-coded sources in forest
    ActorBundle {
        #[arg(default_value = "actor_bundles.car.zst")]
        output: PathBuf,
    },
    /// Generate actors metadata from required bundles list
    GenerateActorsMetadata,
}

impl StateMigrationCommands {
    pub async fn run(self) -> anyhow::Result<()> {
        match self {
            Self::ActorBundle { output } => {
                generate_actor_bundle(&output).await?;
                println!("Wrote the actors bundle to {}", output.display());
                Ok(())
            }
            Self::GenerateActorsMetadata => {
                let metadata = get_actor_bundles_metadata().await?;
                let metadata_json = serde_json::to_string_pretty(&metadata)?;
                println!("{}", metadata_json);

                Ok(())
            }
        }
    }
}
