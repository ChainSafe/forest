// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use chrono::Utc;
use clap::Subcommand;
use forest_cli_shared::{chain_path, cli::Config};
use forest_db::db_engine::db_root;
use forest_rpc_client::db_ops::db_gc;
use log::error;

use crate::cli::{handle_rpc_err, prompt_confirm};

#[derive(Debug, Subcommand)]
pub enum DBCommands {
    /// Show DB stats
    Stats,
    /// Run DB garbage collection
    GC,
    /// DB Clean up
    Clean {
        /// Answer yes to all forest-cli yes/no questions without prompting
        #[arg(long)]
        force: bool,
    },
}

impl DBCommands {
    pub async fn run(&self, config: &Config) -> anyhow::Result<()> {
        match self {
            Self::Stats => {
                use human_repr::HumanCount;

                let dir = db_root(&chain_path(config));
                println!("Database path: {}", dir.display());
                let size = fs_extra::dir::get_size(dir).unwrap_or_default();
                println!("Database size: {}", size.human_count_bytes());
                Ok(())
            }
            Self::GC => {
                let start = Utc::now();

                db_gc((), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;

                println!(
                    "DB GC completed. took {}s",
                    (Utc::now() - start).num_seconds()
                );

                Ok(())
            }
            Self::Clean { force } => {
                let dir = db_root(&chain_path(config));
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
