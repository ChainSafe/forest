// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::ffi::OsString;

use crate::cli_shared::logger;
use crate::daemon::get_actual_chain_name;
use crate::shim::address::{CurrentNetwork, Network};
use crate::utils::bail_moved_cmd;
use crate::{cli::subcommands::Cli, rpc_client::state_network_name};
use clap::Parser;

use super::subcommands::Subcommand;

pub fn main<ArgT>(args: impl IntoIterator<Item = ArgT>) -> anyhow::Result<()>
where
    ArgT: Into<OsString> + Clone,
{
    // Capture Cli inputs
    let Cli { token, cmd } = Cli::parse_from(args);

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let mut config = crate::Config::default();
            config.client.rpc_token = token;
            logger::setup_logger(&crate::cli_shared::cli::CliOpts::default());
            if let Ok(name) = state_network_name((), &config.client.rpc_token).await {
                if get_actual_chain_name(&name) != "mainnet" {
                    CurrentNetwork::set_global(Network::Testnet);
                }
            }
            // Run command
            match cmd {
                Subcommand::Fetch(_cmd) => {
                    bail_moved_cmd("fetch-params", "forest-tool fetch-params")
                }
                Subcommand::Chain(cmd) => cmd.run(config).await,
                Subcommand::Auth(cmd) => cmd.run(config).await,
                Subcommand::Net(cmd) => cmd.run(config).await,
                Subcommand::Wallet(..) => bail_moved_cmd("wallet", "forest-wallet"),
                Subcommand::Sync(cmd) => cmd.run(config).await,
                Subcommand::Mpool(cmd) => cmd.run(config).await,
                Subcommand::State(cmd) => cmd.run(config).await,
                Subcommand::Config(cmd) => cmd.run(&config, &mut std::io::stdout()),
                Subcommand::Send(cmd) => cmd.run(config).await,
                Subcommand::Info(cmd) => cmd.run(config).await,
                Subcommand::DB(cmd) => cmd.run().await,
                Subcommand::Snapshot(cmd) => cmd.run(config).await,
                Subcommand::Archive(cmd) => cmd.run().await,
                Subcommand::Attach(cmd) => cmd.run(config),
                Subcommand::Shutdown(cmd) => cmd.run(config).await,
                Subcommand::Car(..) => bail_moved_cmd("car", "forest-tool"),
            }
        })
}
