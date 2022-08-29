// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod cli;
mod daemon;
mod logger;
mod subcommand;

use cli::{cli_error_and_die, Cli};

use async_std::task;
use daemonize_me::{Daemon, Group, User};
use log::info;
use structopt::StructOpt;

use std::fs::File;

fn main() {
    logger::setup_logger();
    // Capture Cli inputs
    let Cli { opts, cmd } = Cli::from_args();

    // Run forest as a daemon if no other subcommands are used. Otherwise, run the subcommand.
    match opts.to_config() {
        Ok(cfg) => match cmd {
            Some(command) => {
                task::block_on(subcommand::process(command, cfg));
            }
            None => {
                if opts.detach {
                    let stdout = File::create("stdout.log").unwrap();
                    let stderr = File::create("stderr.log").unwrap();
                    let daemon = Daemon::new()
                        .pid_file("forest.pid", Some(false))
                        .user(User::try_from("guillaume").unwrap())
                        .group(Group::try_from("staff").unwrap())
                        .umask(0o027)
                        .work_dir(".")
                        .stdout(stdout)
                        .stderr(stderr)
                        .start();

                    match daemon {
                        Ok(_) => info!("Daemonized with success"),
                        Err(e) => {
                            cli_error_and_die(&format!("Error daemonizing. Error was: {}", e), 1);
                        }
                    }
                }
                task::block_on(daemon::start(cfg));
            }
        },
        Err(e) => {
            cli_error_and_die(&format!("Error parsing config. Error was: {}", e), 1);
        }
    };
}
