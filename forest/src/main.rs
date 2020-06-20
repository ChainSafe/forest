// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod cli;
mod daemon;
mod logger;
pub(crate) mod paramfetch;
mod subcommand;

use cli::CLI;
use structopt::StructOpt;

fn main() {
    logger::setup_logger();

    // Capture CLI inputs
    match CLI::from_args() {
        CLI {
            daemon_opts,
            cmd: None,
        } => daemon::start(daemon_opts.to_config().unwrap()),
        CLI {
            cmd: Some(command), ..
        } => subcommand::process(command),
    }
}
