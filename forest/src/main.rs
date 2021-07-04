// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod cli;
mod daemon;
mod logger;
mod subcommand;

use cli::{cli_config, CLI};
use structopt::StructOpt;

#[async_std::main]
async fn main() {
    logger::setup_logger();
    // Capture CLI inputs
    match CLI::from_args() {
        CLI { opts, cmd: None } => daemon::start(opts.to_config().unwrap()).await,
        CLI {
            opts,
            cmd: Some(command),
        } => {
            cli_config(opts).await;
            subcommand::process(command).await;
        }
    }
}
