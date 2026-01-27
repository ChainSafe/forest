// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state_cmd;

use crate::cli_shared::cli::HELP_MESSAGE;
use crate::rpc::Client;
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
    /// Fetch RPC test snapshots to the local cache
    FetchRpcTests,
    #[command(subcommand)]
    State(state_cmd::StateCommand),
}

impl Subcommand {
    pub async fn run(self, _client: Client) -> anyhow::Result<()> {
        match self {
            Self::FetchRpcTests => fetch_rpc_tests().await,
            Self::State(cmd) => cmd.run().await,
        }
    }
}

async fn fetch_rpc_tests() -> anyhow::Result<()> {
    crate::utils::proofs_api::maybe_set_proofs_parameter_cache_dir_env(
        &crate::Config::default().client.data_dir,
    );
    ensure_proof_params_downloaded().await?;
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
