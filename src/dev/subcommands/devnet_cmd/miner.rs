// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Miner integration tests (Lotus Miner or Curio via `MINER_BACKEND`).

mod api;
mod curio;
mod devnet;
mod onboarding;
mod wdpost;

use crate::dev::subcommands::tests_cmd::helpers::harness_env;
use crate::lotus_json::HasLotusJson;
use crate::rpc::{Client, Request, humanize_rpc_error};
use anyhow::{Context as _, bail};
use clap::ValueEnum as _;
use libtest_mimic::{Arguments, Trial};
use std::fmt::Debug;

/// Miner integration tests on Forest (Lotus Miner or Curio; set `MINER_BACKEND` in the stack `.env`)
#[derive(Debug, clap::Args)]
pub struct MinerTestCommand {
    /// Run only this group; all groups run by default
    #[arg(long)]
    group: Option<Group>,
    /// Run only the tests whose name contains this string
    filter: Option<String>,
}

/// CI runs each group as its own job/devnet.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum Group {
    Onboarding,
    Wdpost,
}

impl MinerTestCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        let _ = MinerBackend::from_env()?;
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
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MinerBackend {
    Lotus,
    Curio,
}

impl MinerBackend {
    pub fn from_env() -> anyhow::Result<Self> {
        let backend = harness_env("MINER_BACKEND")?;
        match backend.as_str() {
            "lotus" => Ok(Self::Lotus),
            "curio" => Ok(Self::Curio),
            _ => bail!("MINER_BACKEND must be lotus or curio (got `{backend}`)"),
        }
    }
}

/// `FULLNODE_API_INFO` shell fragment for `docker exec` (token + `/dns/forest/...`).
pub(super) fn forest_fullnode_api_info_shell() -> anyhow::Result<String> {
    let port = harness_env("FOREST_RPC_PORT")?;
    let data_dir = harness_env("FOREST_DATA_DIR")?;
    Ok(format!(
        "$(cat '{data_dir}/token.jwt'):/dns/forest/tcp/{port}/http"
    ))
}

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
