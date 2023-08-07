// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::db::db_engine::db_root;
use clap::Subcommand;
use std::path::PathBuf;
use tracing::error;

use crate::cli::subcommands::prompt_confirm;

#[derive(Debug, Subcommand)]
pub enum DBCommands {
    /// Show DB stats
    Stats {
        #[arg(long)]
        chain_path: PathBuf,
    },
    /// DB Clean up
    Clean {
        /// Answer yes to all forest-cli yes/no questions without prompting
        #[arg(long)]
        force: bool,
        #[arg(long)]
        chain_path: PathBuf,
    },
}

impl DBCommands {
    pub async fn run(&self) -> anyhow::Result<()> {
        match self {
            Self::Stats { chain_path } => {
                use human_repr::HumanCount;

                let dir = db_root(&chain_path);
                if !dir.is_dir() {
                    println!(
                        "Aborted. Database path {} is not a valid directory",
                        dir.display()
                    );
                    return Ok(());
                }
                println!("Database path: {}", dir.display());
                let size = fs_extra::dir::get_size(dir).unwrap_or_default();
                println!("Database size: {}", size.human_count_bytes());
                Ok(())
            }
            Self::Clean { force, chain_path } => {
                let dir = chain_path;
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
