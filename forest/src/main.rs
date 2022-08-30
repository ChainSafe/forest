// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod cli;
mod daemon;
mod logger;
mod subcommand;

use cli::{cli_error_and_die, Cli, DaemonConfig};

use async_std::task;
use daemonize_me::{Daemon, DaemonError, Group, User};
use log::info;
use structopt::StructOpt;

use std::fs::File;

fn build_daemon<'a>(config: &DaemonConfig) -> Result<Daemon<'a>, DaemonError> {
    let daemon = Daemon::new();
    let daemon = if let Some(user) = &config.user {
        daemon.user(User::try_from(user)?)
    } else {
        daemon
    };
    let daemon = if let Some(group) = &config.group {
        daemon.group(Group::try_from(group)?)
    } else {
        daemon
    };
    let daemon = daemon.umask(config.umask);
    let daemon = if let Some(path) = &config.stdout {
        let file = File::create(path).expect("File creation {path} must succeed");
        daemon.stdout(file)
    } else {
        daemon
    };
    let daemon = if let Some(path) = &config.stderr {
        let file = File::create(path).expect("File creation {path} must succeed");
        daemon.stderr(file)
    } else {
        daemon
    };
    let daemon = daemon.work_dir(&config.work_dir);
    let daemon = if let Some(path) = &config.pid_file {
        daemon.pid_file(path, Some(false))
    } else {
        daemon
    };

    Ok(daemon)
}

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
                    let result = build_daemon(&cfg.daemon)
                        .unwrap_or_else(|e| {
                            cli_error_and_die(
                                &format!("Error building daemon. Error was: {}", e),
                                1,
                            )
                        })
                        .start();
                    match result {
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
