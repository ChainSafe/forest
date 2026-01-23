// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::ffi::OsString;

use super::subcommands::Cli;
use super::subcommands::Subcommand;
use crate::cli_shared::logger::setup_minimal_logger;
use clap::Parser as _;

pub async fn main<ArgT>(args: impl IntoIterator<Item = ArgT>) -> anyhow::Result<()>
where
    ArgT: Into<OsString> + Clone,
{
    // Capture Cli inputs
    let Cli { cmd } = Cli::parse_from(args);
    setup_minimal_logger();

    let client = crate::rpc::Client::default_or_from_env(None)?;

    // Run command
    match cmd {
        Subcommand::Backup(cmd) => cmd.run(),
        Subcommand::Benchmark(cmd) => cmd.run().await,
        Subcommand::StateMigration(cmd) => cmd.run().await,
        Subcommand::Snapshot(cmd) => cmd.run().await,
        Subcommand::Fetch(cmd) => cmd.run().await,
        Subcommand::Archive(cmd) => cmd.run().await,
        Subcommand::DB(cmd) => cmd.run().await,
        Subcommand::Index(cmd) => cmd.run().await,
        Subcommand::Car(cmd) => cmd.run().await,
        Subcommand::Api(cmd) => cmd.run().await,
        Subcommand::Net(cmd) => cmd.run().await,
        Subcommand::Shed(cmd) => cmd.run(client).await,
        Subcommand::Completion(cmd) => cmd.run(&mut std::io::stdout()),
    }
}
