// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::rpc_api::progress_api::GetProgressType;
use crate::rpc_client::ApiInfo;
use crate::utils::io::ProgressBar;
use chrono::Utc;
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum DBCommands {
    /// Run DB garbage collection
    GC,
}

impl DBCommands {
    pub async fn run(self, api: ApiInfo) -> anyhow::Result<()> {
        match self {
            Self::GC => {
                let start = Utc::now();

                let bar = Arc::new(tokio::sync::Mutex::new({
                    let bar = ProgressBar::new(0);
                    bar.message("Running database garbage collection | blocks ");
                    bar
                }));
                tokio::spawn({
                    let bar = bar.clone();
                    let api = api.clone();
                    async move {
                        let mut interval =
                            tokio::time::interval(tokio::time::Duration::from_secs(1));
                        loop {
                            interval.tick().await;
                            if let Ok((progress, total)) = api
                                .get_progress(GetProgressType::DatabaseGarbageCollection)
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

                api.db_gc().await?;

                bar.lock().await.finish_println(&format!(
                    "Database garbage collection completed. took {}s",
                    (Utc::now() - start).num_seconds()
                ));

                Ok(())
            }
        }
    }
}
