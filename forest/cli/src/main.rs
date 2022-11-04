// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod cli;
mod subcommand;

use cli::{cli_error_and_die, Cli};

use async_std::task;
use forest_cli_shared::{cli::LogConfig, logger};
use structopt::StructOpt;

fn main() {
    // Capture Cli inputs
    let Cli { opts, cmd } = Cli::from_args();

    // Run forest as a daemon if no other subcommands are used. Otherwise, run the subcommand.
    match opts.to_config() {
        Ok((cfg, _)) => {
            logger::setup_logger(&cfg.log, opts.color.into());
            task::block_on(subcommand::process(cmd, cfg));
        }
        Err(e) => {
            logger::setup_logger(&LogConfig::default(), opts.color.into());
            cli_error_and_die(format!("Error parsing config: {e}"), 1);
        }
    };
}
