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
use raw_sync::{events::*, Timeout};
use shared_memory::{ShmemConf, ShmemError};
use structopt::StructOpt;

use std::fs::File;
use std::{thread, time};

const SHMEM_PATH: &str = "shmem-mapping";

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

    daemon = daemon.setup_post_fork_parent_hook(|_parent_pid, _child_pid| {
        let shmem = ShmemConf::new()
            .flink(SHMEM_PATH)
            .open()
            .expect("open must succeed");

        info!("Creating event in shared memory");
        let (event, _) =
            unsafe { Event::new(shmem.as_ptr(), true).expect("Even::new must succeed") };
        event.wait(Timeout::Infinite).expect("wait must succeed");

        info!("Exiting");
        drop(shmem);

        std::process::exit(0);
    });

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
                let shmem = ShmemConf::new()
                    .size(4096)
                    .flink(SHMEM_PATH)
                    .create()
                    .expect("shmem must succeed");

                if opts.detach {
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
                }
                task::block_on(daemon::start(cfg, SHMEM_PATH));
            }
        },
        Err(e) => {
            cli_error_and_die(format!("Error parsing config. Error was: {e}"), 1);
        }
    };
}
