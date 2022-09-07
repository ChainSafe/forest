// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod cli;
mod daemon;
mod logger;
mod subcommand;

use cli::{cli_error_and_die, Cli, DaemonConfig};

use async_std::task;
use daemonize_me::{Daemon, Group, User};
use lazy_static::lazy_static;
use log::info;
use raw_sync::events::{Event, EventInit};
use raw_sync::Timeout;
use shared_memory::ShmemConf;
use structopt::StructOpt;

    daemon = daemon.setup_post_fork_parent_hook(|_parent_pid, _child_pid| {
        let mut shmem = ipc_shmem_conf().open().expect("open must succeed");
        shmem.set_owner(true);
        let (event, _) =
            unsafe { Event::from_existing(shmem.as_ptr()).expect("from_existing must succeed") };
        let ret = event.wait(EVENT_TIMEOUT);
        drop(shmem); // Delete the local link and the shared memory object.
        if ret.is_err() {
            cli_error_and_die(
                "Error unblocking process. Check the log file for details.",
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
                    info!(
                        "Redirecting stdout and stderr to files {} and {}.",
                        cfg.daemon.stdout.display(),
                        cfg.daemon.stderr.display()
                    );
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
                task::block_on(daemon::start(cfg, opts.detach));
            }
        },
        Err(e) => {
            cli_error_and_die(format!("Error parsing config. Error was: {e}"), 1);
        }
    };
}
