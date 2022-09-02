// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod cli;
mod daemon;
mod logger;
mod subcommand;

use cli::{cli_error_and_die, Cli, DaemonConfig};

use async_std::task;
use daemonize_me::{Daemon, DaemonError, Group, User};
use log::{info, warn};
use raw_sync::{events::*, Timeout};
use shared_memory::{Shmem, ShmemConf};
use structopt::StructOpt;

use std::fs::File;
use std::mem;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::time::Duration;

static SHMEM_PTR: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());

const EVENT_TIMEOUT: Timeout = Timeout::Val(Duration::from_secs(4));

fn create_event() -> Shmem {
    let shmem = ShmemConf::new()
        .size(mem::size_of::<Event>())
        .create()
        .expect("create must succeed");
    SHMEM_PTR.store(shmem.as_ptr(), Ordering::Relaxed);
    info!("Creating event in shared memory");
    unsafe {
        Event::new(shmem.as_ptr(), true).expect("Event::new must succeed");
    }
    shmem
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
        let (event, _) = unsafe {
            Event::from_existing(SHMEM_PTR.load(Ordering::Relaxed)).expect("open must succeed")
        };
        match event.wait(EVENT_TIMEOUT) {
            Err(e) => warn!("Event error: {e}"),
            _ => (),
        }

        info!("Exiting");

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
                let _shmem = if opts.detach {
                    let shmem = create_event();
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
                    Some(shmem)
                } else {
                    None
                };
                task::block_on(daemon::start(cfg));
            }
        },
        Err(e) => {
            cli_error_and_die(format!("Error parsing config. Error was: {e}"), 1);
        }
    };
}
