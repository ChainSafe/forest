// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod cli;
mod daemon;
mod logger;
mod subcommand;

use cli::{cli_error_and_die, Cli};
use structopt::StructOpt;
use std::fs::File;
use daemonize_me::{Daemon, User, Group};
use log::{error, info};

#[async_std::main]
async fn main() {
    logger::setup_logger();
    // Capture Cli inputs
    let Cli { opts, cmd } = Cli::from_args();

    // Run forest as a daemon if no other subcommands are used. Otherwise, run the subcommand.
    match opts.to_config() {
        Ok(cfg) => match cmd {
            Some(command) => subcommand::process(command, cfg).await,
            None => {
                if opts.detach {
                    let stdout = File::create("forest.log").unwrap();
                    let stderr = File::create("error.log").unwrap();
                    let daemon = Daemon::new()
                        .pid_file("forest.pid", Some(false))
                        //.user(User::try_from("guillaume").unwrap())
                        .umask(0o027)
                        .work_dir(".")
                        .stdout(stdout)
                        .stderr(stderr)
                        .start();

                    match daemon {
                        Ok(_) => info!("Daemonized with success"),
                        Err(e) => error!("Error daemonizing: {e}"),
                    }
                    daemon::start(cfg).await;
                } else {
                    daemon::start(cfg).await;
                }
            }
        },
        Err(e) => {
            cli_error_and_die(&format!("Error parsing config. Error was: {}", e), 1);
        }
    };
}
