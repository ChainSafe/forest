// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod benchmark_cmd;

use crate::cli_shared::cli::HELP_MESSAGE;
use crate::utils::version::FOREST_VERSION_STRING;
use clap::Parser;

/// Command-line options for the `forest-tool` binary
#[derive(Parser)]
#[command(name = env!("CARGO_PKG_NAME"), author = env!("CARGO_PKG_AUTHORS"), version = FOREST_VERSION_STRING.as_str(), about = env!("CARGO_PKG_DESCRIPTION"))]
#[command(help_template(HELP_MESSAGE))]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Subcommand,
}

/// forest-tool sub-commands
#[derive(clap::Subcommand)]
pub enum Subcommand {
    /// Benchmark various Forest subsystems
    #[command(subcommand)]
    Benchmark(benchmark_cmd::BenchmarkCommands),
}
