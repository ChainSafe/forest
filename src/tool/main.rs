// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::ffi::OsString;

use super::subcommands::Cli;
use crate::cli_shared::logger::setup_minimal_logger;
use clap::Parser;

use super::subcommands::Subcommand;

pub fn main<ArgT>(args: impl IntoIterator<Item = ArgT>) -> anyhow::Result<()>
where
    ArgT: Into<OsString> + Clone,
{
    // Capture Cli inputs
    let Cli { cmd } = Cli::parse_from(args);
    setup_minimal_logger();

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            // Run command
            match cmd {
                Subcommand::Benchmark(cmd) => cmd.run().await,
                Subcommand::StateMigration(state_migration) => state_migration.run().await,
                Subcommand::Snapshot(cmd) => cmd.run().await,
                Subcommand::Fetch(cmd) => cmd.run().await,
                Subcommand::Archive(cmd) => cmd.run().await,
                Subcommand::DB(cmd) => cmd.run().await,
                Subcommand::Car(cmd) => cmd.run().await,
            }
        })
}
