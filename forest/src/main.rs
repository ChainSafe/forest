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
use ipc_channel::ipc::*;

use std::fs::File;

fn build_daemon<'a>(config: &DaemonConfig) -> Result<Daemon<'a>, DaemonError> {
    let mut daemon = Daemon::new().umask(config.umask).work_dir(&config.work_dir);
    if let Some(user) = &config.user {
        daemon = daemon.user(User::try_from(user)?)
    }
    if let Some(group) = &config.group {
        daemon = daemon.group(Group::try_from(group)?)
    }
    if let Some(path) = &config.stdout {
        let file = File::create(path).expect("File creation {path} must succeed");
        daemon = daemon.stdout(file)
    }
    if let Some(path) = &config.stderr {
        let file = File::create(path).expect("File creation {path} must succeed");
        daemon = daemon.stderr(file)
    }
    if let Some(path) = &config.pid_file {
        daemon = daemon.pid_file(path, Some(false))
    }

    daemon = daemon.setup_post_fork_parent_hook(|parent_pid, child_pid| {
        info!("{parent_pid}: I'm your father {child_pid}");

        let (_, rx): (IpcSender<()>, IpcReceiver<()>) = channel().unwrap();

        loop {
            match rx.try_recv() {
                Ok(_) => {
                    // Do something interesting with your result
                    break;
                },
                Err(_) => {
                    // Do something else useful while we wait
                    ()
                }
            }
        }
        info!("Exiting");
        std::process::exit(0);
    });

    Ok(daemon)
}

fn main() {
    logger::setup_logger();
    let (server, name) = IpcOneShotServer::<()>::new().unwrap();
    info!("IPC server {name} created");

    // Capture Cli inputs
    let Cli { opts, cmd } = Cli::from_args();

    // Run forest as a daemon if no other subcommands are used. Otherwise, run the subcommand.
    match opts.to_config() {
        Ok(cfg) => match cmd {
            Some(command) => {
                task::block_on(subcommand::process(command, cfg));
            }
            None => {
                let name = if opts.detach {
                    let result = build_daemon(&cfg.daemon)
                        .unwrap_or_else(|e| {
                            cli_error_and_die(format!("Error building daemon. Error was: {e}"), 1)
                        })
                        .start();
                    match result {
                        Ok(_) => info!("Process detached"),
                        Err(e) => {
                            cli_error_and_die(format!("Error when detaching. Error was: {e}"), 1);
                        }
                    }
                    Some(name)
                } else {
                    None
                };
                task::block_on(daemon::start(cfg, name));
            }
        },
        Err(e) => {
            cli_error_and_die(format!("Error parsing config. Error was: {e}"), 1);
        }
    };
}
