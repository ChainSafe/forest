// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Lotus Miner integration tests, ported from Lotus `itests`. They need
//! `scripts/devnet-forest-miner`, where Lotus Miner seals and mines through Forest's RPC.

mod api;
mod devnet;
mod onboarding;
mod terminate;
mod wdpost;

use crate::dev::subcommands::tests_cmd::helpers::harness_env;
use crate::lotus_json::HasLotusJson;
use crate::rpc::{Client, Request, humanize_rpc_error};
use anyhow::{Context as _, ensure};
use clap::ValueEnum as _;
use libtest_mimic::{Arguments, Trial};
use std::fmt::Debug;
use std::path::Path;

/// Lotus Miner integration tests on Forest (`scripts/devnet-forest-miner` only)
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
    Wdpost,
    Terminate,
}

impl MinerTestCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let devnet_dir = harness_env("DEVNET_DIR")?;
        ensure!(
            Path::new(&devnet_dir).ends_with("devnet-forest-miner"),
            "`devnet miner` drives lotus-miner over Forest's RPC, which only scripts/devnet-forest-miner does; DEVNET_DIR is `{devnet_dir}`"
        );
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
        Group::Wdpost => wdpost::tests(),
        Group::Terminate => terminate::tests(),
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
