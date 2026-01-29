// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub(crate) mod api_cmd;
pub(crate) mod archive_cmd;
mod backup_cmd;
mod benchmark_cmd;
mod car_cmd;
mod db_cmd;
mod fetch_params_cmd;
mod index_cmd;
mod net_cmd;
mod shed_cmd;
mod snapshot_cmd;
mod state_migration_cmd;

use crate::cli_shared::cli::*;
use crate::cli_shared::cli::{CompletionCommand, HELP_MESSAGE};
use crate::utils::version::FOREST_VERSION_STRING;
use clap::Parser;

/// Command-line options for the `forest-tool` binary
#[derive(Parser)]
#[command(name = env!("CARGO_PKG_NAME"), bin_name = "forest-tool", author = env!("CARGO_PKG_AUTHORS"), version = FOREST_VERSION_STRING.as_str(), about = env!("CARGO_PKG_DESCRIPTION")
)]
#[command(help_template(HELP_MESSAGE))]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Subcommand,
}

/// forest-tool sub-commands
#[derive(clap::Subcommand)]
#[allow(clippy::large_enum_variant)]
pub enum Subcommand {
    /// Create and restore backups
    #[command(subcommand)]
    Backup(backup_cmd::BackupCommands),

    /// Benchmark various Forest subsystems
    #[command(subcommand)]
    Benchmark(benchmark_cmd::BenchmarkCommands),

    /// State migration tools
    #[command(subcommand)]
    StateMigration(state_migration_cmd::StateMigrationCommands),

    /// Manage snapshots
    #[command(subcommand)]
    Snapshot(snapshot_cmd::SnapshotCommands),

    /// Download parameters for generating and verifying proofs for given size
    #[command(name = "fetch-params")]
    Fetch(fetch_params_cmd::FetchCommands),

    /// Manage archives
    #[command(subcommand)]
    Archive(archive_cmd::ArchiveCommands),

    /// Database management
    #[command(subcommand)]
    DB(db_cmd::DBCommands),

    /// Index database management
    #[command(subcommand)]
    Index(index_cmd::IndexCommands),

    /// Utilities for manipulating CAR files
    #[command(subcommand)]
    Car(car_cmd::CarCommands),

    /// API tooling
    #[command(subcommand)]
    Api(api_cmd::ApiCommands),

    /// Network utilities
    #[command(subcommand)]
    Net(net_cmd::NetCommands),

    /// Miscellaneous, semver-exempt commands for developer use.
    #[command(subcommand)]
    Shed(shed_cmd::ShedCommands),

    Completion(CompletionCommand),
}
