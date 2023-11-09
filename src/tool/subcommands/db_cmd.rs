// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::cli::subcommands::prompt_confirm;
use crate::cli_shared::{chain_path, read_config};
use crate::db::db_engine::db_root;
use crate::networks::NetworkChain;
use clap::Subcommand;
use tracing::error;

#[derive(Debug, Subcommand)]
pub enum DBCommands {
    /// Show DB stats
    Stats {
        /// Optional TOML file containing forest daemon configuration
        #[arg(short, long)]
        config: Option<String>,
        /// Optional chain, will override the chain section of configuration file if used
        #[arg(long)]
        chain: Option<NetworkChain>,
    },
    /// DB destruction
    Destroy {
        /// Answer yes to all forest-cli yes/no questions without prompting
        #[arg(long)]
        force: bool,
        /// Optional TOML file containing forest daemon configuration
        #[arg(short, long)]
        config: Option<String>,
        /// Optional chain, will override the chain section of configuration file if used
        #[arg(long)]
        chain: Option<NetworkChain>,
    },
}

impl DBCommands {
    pub async fn run(&self) -> anyhow::Result<()> {
        match self {
            Self::Stats { config, chain } => {
                use human_repr::HumanCount;

                let (_, config) = read_config(config, chain)?;

                let dir = db_root(&chain_path(&config))?;
                println!("Database path: {}", dir.display());
                let size = fs_extra::dir::get_size(dir).unwrap_or_default();
                println!("Database size: {}", size.human_count_bytes());
                Ok(())
            }
            Self::Destroy {
                force,
                config,
                chain,
            } => {
                let (_, config) = read_config(config, chain)?;

                let dir = chain_path(&config);
                if !dir.is_dir() {
                    println!(
                        "Aborted. Database path {} is not a valid directory",
                        dir.display()
                    );
                    return Ok(());
                }
                println!("Deleting {}", dir.display());
                if !force && !prompt_confirm() {
                    println!("Aborted.");
                    return Ok(());
                }
                match fs_extra::dir::remove(&dir) {
                    Ok(_) => {
                        println!("Deleted {}", dir.display());
                        Ok(())
                    }
                    Err(err) => {
                        error!("{err}");
                        Ok(())
                    }
                }
            }
        }
    }
}
