// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use forest_cli_shared::{chain_path, cli::Config};
use forest_db::db_engine::db_path;
use log::error;
use structopt::StructOpt;

use crate::cli::prompt_confirm;

#[derive(Debug, StructOpt)]
pub enum DBCommands {
    /// Show DB stats
    Stats,
    /// DB Clean up
    Clean {
        /// Answer yes to all forest-cli yes/no questions without prompting
        #[structopt(long)]
        force: bool,
    },
}

impl DBCommands {
    pub async fn run(&self, config: &Config) -> anyhow::Result<()> {
        match self {
            Self::Stats => {
                use human_repr::HumanCount;

                let dir = db_path(&chain_path(config));
                println!("Database path: {}", dir.display());
                let size = fs_extra::dir::get_size(dir).unwrap_or_default();
                println!("Database size: {}", size.human_count_bytes());
                Ok(())
            }
            Self::Clean { force } => {
                let dir = db_path(&chain_path(config));
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
