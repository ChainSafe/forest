// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod cli;
mod daemon;
mod logger;
mod subcommand;

use cli::CLI;
use structopt::StructOpt;

#[async_std::main]
async fn main() {
    logger::setup_logger();
    // Capture CLI inputs
    match CLI::from_args() {
        CLI { opts, cmd } => {
            match opts.to_config() {
                Ok(cfg) => match cmd {
                    Some(command) => subcommand::process(command, cfg).await,
                    None => daemon::start(cfg).await,
                },
                Err(e) => {
                    println!("Error parsing config. Error was: {}", e);
                }
            };
        }
    }
}
