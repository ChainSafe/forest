// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::ffi::OsString;

use crate::cli::subcommands::Cli;
use crate::cli_shared::logger;
use crate::daemon::get_actual_chain_name;
use crate::rpc_client::ApiInfo;
use crate::shim::address::{CurrentNetwork, Network};
use crate::utils::bail_moved_cmd;
use clap::Parser;

use super::subcommands::Subcommand;

pub fn main<ArgT>(args: impl IntoIterator<Item = ArgT>) -> anyhow::Result<()>
where
    ArgT: Into<OsString> + Clone,
{
    // Capture Cli inputs
    let Cli { token, cmd } = Cli::parse_from(args);

    let api = ApiInfo::from_env()?.set_token(token);

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            logger::setup_logger(&crate::cli_shared::cli::CliOpts::default());
            if let Ok(name) = api.state_network_name().await {
                if get_actual_chain_name(&name) != "mainnet" {
                    CurrentNetwork::set_global(Network::Testnet);
                }
            }
            // Run command
            match cmd {
                Subcommand::Fetch(_cmd) => {
                    bail_moved_cmd("fetch-params", "forest-tool fetch-params")
                }
                Subcommand::Chain(cmd) => cmd.run(api).await,
                Subcommand::Auth(cmd) => cmd.run(api).await,
                Subcommand::Net(cmd) => cmd.run(api).await,
                Subcommand::Wallet(..) => bail_moved_cmd("wallet", "forest-wallet"),
                Subcommand::Sync(cmd) => cmd.run(api).await,
                Subcommand::Mpool(cmd) => cmd.run(api).await,
                Subcommand::State(cmd) => cmd.run(api).await,
                Subcommand::Config(cmd) => cmd.run(&mut std::io::stdout()),
                Subcommand::Send(cmd) => cmd.run(api).await,
                Subcommand::Info(cmd) => cmd.run(api).await,
                Subcommand::DB(cmd) => cmd.run(api).await,
                Subcommand::Snapshot(cmd) => cmd.run(api).await,
                Subcommand::Archive(cmd) => cmd.run().await,
                Subcommand::Attach(cmd) => cmd.run(api),
                Subcommand::Shutdown(cmd) => cmd.run(api).await,
                Subcommand::Car(..) => bail_moved_cmd("car", "forest-tool"),
            }
        })
}
