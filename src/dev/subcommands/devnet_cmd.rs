// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Integration suites that run against the local docker devnet. Unlike the unit
//! suite, these need a running devnet with both a Forest and a Lotus node reachable and the
//! test harness environment wired up. [`preflight`] fails early with actionable errors when
//! that environment is missing, rather than letting a suite surface it as an opaque mid-run error.

mod eth_gas;

use crate::dev::subcommands::tests_cmd::helpers::{docker, forest_client, lotus_client};
use crate::rpc::prelude::*;
use anyhow::{Context as _, ensure};

/// Integration tests that require the docker devnet
#[derive(Debug, clap::Subcommand)]
pub enum DevnetCommand {
    EthGas(eth_gas::EthGasTestCommand),
}

impl DevnetCommand {
    pub async fn run(self) -> anyhow::Result<()> {
        preflight().await.context("devnet pre-flight failed")?;
        match self {
            Self::EthGas(cmd) => cmd.run().await,
        }
    }
}

async fn preflight() -> anyhow::Result<()> {
    for container in ["forest", "lotus"] {
        let running = docker(&["inspect", "-f", "{{.State.Running}}", container]).with_context(
            || format!("could not query container `{container}`; is docker running and the local docker devnet up?"),
        )?;
        ensure!(
            running.trim() == "true",
            "devnet container `{container}` is not running; bring the local docker devnet up first"
        );
    }

    for var in [
        "FULLNODE_API_INFO",
        "FOREST_TEST_PRELOADED_ADDRESS",
        "LOTUS_RPC_PORT",
    ] {
        ensure!(
            std::env::var_os(var).is_some(),
            "{var} is not set; source the devnet test harness and run `devnet_test_env_init` first"
        );
    }

    // Probe eth RPC specifically, not just `ChainHead`: the suites need it, and a devnet with eth
    // RPC disabled would otherwise pass here and fail opaquely mid-suite.
    for (node, client) in [("forest", forest_client()?), ("lotus", lotus_client()?)] {
        EthBlockNumber::call(&client, ()).await.with_context(|| {
            format!("{node} eth RPC is not reachable; is the local docker devnet up (with eth RPC enabled) and synced?")
        })?;
    }

    Ok(())
}
