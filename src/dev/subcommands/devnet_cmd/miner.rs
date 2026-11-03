// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Miner integration tests on Forest, against Lotus Miner or Curio.

mod api;
mod curio;
mod devnet;
mod onboarding;

use crate::dev::subcommands::tests_cmd::helpers::harness_env;
use crate::lotus_json::HasLotusJson;
use crate::rpc::{Client, Request, humanize_rpc_error};
use anyhow::{Context as _, ensure};
use clap::ValueEnum as _;
use libtest_mimic::{Arguments, Trial};
use std::fmt::Debug;
use std::path::Path;

/// Miner integration tests on Forest (`scripts/devnet-forest-miner` or `scripts/devnet-forest-curio`)
#[derive(Debug, clap::Args)]
pub struct MinerTestCommand {
    /// Run only this group; all groups run by default
    #[arg(long)]
    group: Option<Group>,
    /// Run only the tests whose name contains this string
    filter: Option<String>,
}

/// Tests that share one devnet. CI runs each group as a separate job on its own devnet.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum Group {
    Onboarding,
}

impl MinerTestCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let _ = on_curio()?;
        let groups = match self.group {
            Some(group) => vec![group],
            None => Group::value_variants().to_vec(),
        };
        let args = Arguments {
            filter: self.filter,
            test_threads: Some(1),
            ..Default::default()
        };
        libtest_mimic::run(&args, groups.into_iter().flat_map(tests).collect()).exit();
    }
}

fn tests(group: Group) -> Vec<Trial> {
    match group {
        Group::Onboarding => onboarding::tests(),
    }
}

/// Whether `DEVNET_DIR` is the Curio stack (`devnet-forest-curio`); errors on any other directory.
pub(super) fn on_curio() -> anyhow::Result<bool> {
    let devnet_dir = harness_env("DEVNET_DIR")?;
    let path = Path::new(&devnet_dir);
    if path.ends_with("devnet-forest-curio") {
        Ok(true)
    } else {
        ensure!(
            path.ends_with("devnet-forest-miner"),
            "`devnet miner` needs scripts/devnet-forest-miner or scripts/devnet-forest-curio; DEVNET_DIR is `{devnet_dir}`"
        );
        Ok(false)
    }
}

/// Calls `request` on `node`, naming the node and the method in the error.
async fn call<T: HasLotusJson + Debug>(
    client: &Client,
    node: &str,
    request: Request<T>,
) -> anyhow::Result<T> {
    let method = request.method_name.clone();
    client
        .call(request)
        .await
        .with_context(|| format!("{node} {method}"))
        .map_err(humanize_rpc_error)
}
