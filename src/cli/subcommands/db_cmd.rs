// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::cli_shared::cli::Config;
use crate::rpc_api::progress_api::GetProgressType;
use crate::rpc_client::{db_ops::db_gc, progress_ops::get_progress};
use crate::utils::io::ProgressBar;
use chrono::Utc;
use clap::Subcommand;

use crate::cli::subcommands::handle_rpc_err;

#[derive(Debug, Subcommand)]
pub enum DBCommands {
    /// Run DB garbage collection
    GC,
    // Those subcommands are hidden and only here to help users migrating to forest-tool
    #[command(hide = true)]
    Stats,
    #[command(hide = true)]
    Clean {
        #[arg(long)]
        force: bool,
    },
}

impl DBCommands {
    pub async fn run(&self, config: &Config) -> anyhow::Result<()> {
        match self {
            Self::Stats => crate::bail_moved_cmd!("db stats"),
            Self::GC => {
                let start = Utc::now();

                let bar = Arc::new(tokio::sync::Mutex::new({
                    let bar = ProgressBar::new(0);
                    bar.message("Running database garbage collection | blocks ");
                    bar
                }));
                tokio::spawn({
                    let bar = bar.clone();
                    async move {
                        let mut interval =
                            tokio::time::interval(tokio::time::Duration::from_secs(1));
                        loop {
                            interval.tick().await;
                            if let Ok((progress, total)) =
                                get_progress((GetProgressType::DatabaseGarbageCollection,), &None)
                                    .await
                            {
                                let bar = bar.lock().await;
                                if bar.is_finish() {
                                    break;
                                }
                                bar.set_total(total);
                                bar.set(progress);
                            }
                        }
                    }
                });

                db_gc((), &config.client.rpc_token)
                    .await
                    .map_err(handle_rpc_err)?;

                bar.lock().await.finish_println(&format!(
                    "Database garbage collection completed. took {}s",
                    (Utc::now() - start).num_seconds()
                ));

                Ok(())
            }
            Self::Clean { force: _ } => crate::bail_moved_cmd!("db clean", "db destroy"),
        }
    }
}
