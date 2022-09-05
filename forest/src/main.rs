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
use raw_sync::events::{Event, EventInit};
use raw_sync::Timeout;
use shared_memory::ShmemConf;
use structopt::StructOpt;

use std::fs::File;
use std::process;
use std::time::Duration;

const EVENT_TIMEOUT: Timeout = Timeout::Val(Duration::from_secs(20));

// The parent process and the daemonized child communicate through an Event in
// shared memory. The identity of the shared memory object is written to a local
// file named .forest_daemon_ipc. The parent process is responsible for cleaning
// up the local file and the shared memory object.
fn ipc_shmem_conf() -> ShmemConf {
    ShmemConf::new()
        .size(Event::size_of(None))
        .force_create_flink()
        .flink(".forest_daemon_ipc")
}

// Initiate an Event object in shared memory.
fn create_ipc_lock() {
    let mut shmem = ipc_shmem_conf().create().expect("create must succeed");
    // The shared memory object will not be deleted when 'shmen' is dropped
    // because we're not the owner.
    shmem.set_owner(false);
    unsafe {
        Event::new(shmem.as_ptr(), true).expect("new must succeed");
    }
}

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
        let mut shmem = ipc_shmem_conf().open().expect("open must succeed");
        shmem.set_owner(true);
        let (event, _) =
            unsafe { Event::from_existing(shmem.as_ptr()).expect("from_existing must succeed") };
        let ret = event.wait(EVENT_TIMEOUT);
        drop(shmem); // Delete the local link and the shared memory object.
        if let Err(e) = ret {
            cli_error_and_die(
                format!(
                    "Error unblocking process. Error was: {e}. Check the log file for details."
                ),
                1,
            );
        }
        info!("Forest has been detached and runs in the background.");
        process::exit(0);
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
                if opts.detach {
                    create_ipc_lock();
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
                task::block_on(daemon::start(cfg));
            }
        },
        Err(e) => {
            cli_error_and_die(format!("Error parsing config. Error was: {e}"), 1);
        }
    };
}
