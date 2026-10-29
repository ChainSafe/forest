// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Lotus Miner integration tests, ported from Lotus `itests`. They need
//! `scripts/devnet-forest-miner`, where Lotus Miner seals and mines through Forest's RPC.

mod api;
mod devnet;
mod onboarding;
mod wdpost;

use crate::dev::subcommands::tests_cmd::helpers::harness_env;
use anyhow::ensure;
use clap::ValueEnum as _;
use libtest_mimic::{Arguments, Trial};
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
    }
}
