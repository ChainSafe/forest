// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state_cmd;
mod update_checkpoints_cmd;

use crate::cli_shared::cli::HELP_MESSAGE;
use crate::networks::generate_actor_bundle;
use crate::rpc::Client;
use crate::state_manager::utils::state_compute::{
    get_state_snapshot_file, list_state_snapshot_files,
};
use crate::utils::net::{DownloadFileOption, download_file_with_cache};
use crate::utils::proofs_api::ensure_proof_params_downloaded;
use crate::utils::version::FOREST_VERSION_STRING;
use anyhow::Context as _;
use clap::Parser;
use directories::ProjectDirs;
use std::borrow::Cow;
use std::path::PathBuf;
use std::time::Duration;
use tokio::task::JoinSet;
use url::Url;

/// Command-line options for the `forest-dev` binary
#[derive(Parser)]
#[command(name = env!("CARGO_PKG_NAME"), bin_name = "forest-dev", author = env!("CARGO_PKG_AUTHORS"), version = FOREST_VERSION_STRING.as_str(), about = env!("CARGO_PKG_DESCRIPTION")
)]
#[command(help_template(HELP_MESSAGE))]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Subcommand,
}

/// forest-dev sub-commands
#[derive(clap::Subcommand)]
pub enum Subcommand {
    /// Fetch test snapshots to the local cache
    FetchTestSnapshots {
        // Save actor bundle to
        #[arg(long)]
        actor_bundle: Option<PathBuf>,
    },
    #[command(subcommand)]
    State(state_cmd::StateCommand),
    /// Update known blocks in build/known_blocks.yaml
    UpdateCheckpoints(update_checkpoints_cmd::UpdateCheckpointsCommand),
}

impl Subcommand {
    pub async fn run(self, _client: Client) -> anyhow::Result<()> {
        match self {
            Self::FetchTestSnapshots { actor_bundle } => fetch_test_snapshots(actor_bundle).await,
            Self::State(cmd) => cmd.run().await,
            Self::UpdateCheckpoints(cmd) => cmd.run().await,
        }
    }
}

async fn fetch_test_snapshots(actor_bundle: Option<PathBuf>) -> anyhow::Result<()> {
    // Prepare proof parameter files
    crate::utils::proofs_api::maybe_set_proofs_parameter_cache_dir_env(
        &crate::Config::default().client.data_dir,
    );
    ensure_proof_params_downloaded().await?;

    // Prepare actor bundles
    if let Some(actor_bundle) = actor_bundle {
        generate_actor_bundle(&actor_bundle).await?;
        println!("Wrote the actors bundle to {}", actor_bundle.display());
    }

    // Prepare state computation and validation snapshots
    fetch_state_tests().await?;

    // Prepare RPC test snapshots
    fetch_rpc_tests().await?;

    Ok(())
}

pub async fn fetch_state_tests() -> anyhow::Result<()> {
    let files = list_state_snapshot_files().await?;
    let mut joinset = JoinSet::new();
    for file in files {
        joinset.spawn(async move { get_state_snapshot_file(&file).await });
    }
    for result in joinset.join_all().await {
        if let Err(e) = result {
            tracing::warn!("{e}");
        }
    }
    Ok(())
}

async fn fetch_rpc_tests() -> anyhow::Result<()> {
    let tests = include_str!("../../tool/subcommands/api_cmd/test_snapshots.txt")
        .lines()
        .map(|i| {
            // Remove comment
            i.split("#").next().unwrap().trim().to_string()
        })
        .filter(|l| !l.is_empty() && !l.starts_with('#'));
    let mut joinset = JoinSet::new();
    for test in tests {
        joinset.spawn(fetch_rpc_test_snapshot(test.into()));
    }
    for result in joinset.join_all().await {
        if let Err(e) = result {
            tracing::warn!("{e}");
        }
    }
    Ok(())
}

pub async fn fetch_rpc_test_snapshot<'a>(name: Cow<'a, str>) -> anyhow::Result<PathBuf> {
    let url: Url =
        format!("https://forest-snapshots.fra1.cdn.digitaloceanspaces.com/rpc_test/{name}")
            .parse()
            .with_context(|| format!("Failed to parse URL for test: {name}"))?;
    let project_dir =
        ProjectDirs::from("com", "ChainSafe", "Forest").context("failed to get project dir")?;
    let cache_dir = project_dir.cache_dir().join("test").join("rpc-snapshots");
    let path = crate::utils::retry(
        crate::utils::RetryArgs {
            timeout: Some(Duration::from_secs(30)),
            max_retries: Some(5),
            delay: Some(Duration::from_secs(1)),
        },
        || download_file_with_cache(&url, &cache_dir, DownloadFileOption::NonResumable),
    )
    .await
    .map_err(|e| anyhow::anyhow!("failed to fetch rpc test snapshot {name} :{e}"))?
    .path;
    Ok(path)
}
