// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod cli;
mod daemon;

use cli::Cli;

use daemonize_me::{Daemon, Group, User};
use forest_cli_shared::{
    cli::{check_for_unknown_keys, cli_error_and_die, ConfigPath, DaemonConfig, LogConfig},
    logger,
};
use forest_db::Store;
use forest_utils::io::ProgressBar;
use lazy_static::lazy_static;
use log::{error, info, warn};
use raw_sync::events::{Event, EventInit};
use raw_sync::Timeout;
use shared_memory::ShmemConf;
use structopt::StructOpt;
use tempfile::{Builder, TempPath};
use tokio::runtime::Runtime;

use std::fs::File;
use std::process;
use std::sync::Arc;
use std::time::Duration;

const EVENT_TIMEOUT: Timeout = Timeout::Val(Duration::from_secs(20));

lazy_static! {
    static ref IPC_PATH: TempPath = Builder::new()
        .prefix("forest-ipc")
        .tempfile()
        .expect("tempfile must succeed")
        .into_temp_path();
}

// The parent process and the daemonized child communicate through an Event in
// shared memory. The identity of the shared memory object is written to a
// temporary file. The parent process is responsible for cleaning up the file
// and the shared memory object.
fn ipc_shmem_conf() -> ShmemConf {
    ShmemConf::new()
        .size(Event::size_of(None))
        .force_create_flink()
        .flink(IPC_PATH.as_os_str())
}

// Initiate an Event object in shared memory.
fn create_ipc_lock() {
    let mut shmem = ipc_shmem_conf().create().expect("create must succeed");
    // The shared memory object will not be deleted when 'shmem' is dropped
    // because we're not the owner.
    shmem.set_owner(false);
    unsafe {
        Event::new(shmem.as_ptr(), true).expect("new must succeed");
    }
}

fn build_daemon<'a>(config: &DaemonConfig) -> anyhow::Result<Daemon<'a>> {
    let mut daemon = Daemon::new()
        .umask(config.umask)
        .work_dir(&config.work_dir)
        .stdout(File::create(&config.stdout)?)
        .stderr(File::create(&config.stderr)?);
    if let Some(user) = &config.user {
        daemon = daemon.user(User::try_from(user)?)
    }
    if let Some(group) = &config.group {
        daemon = daemon.group(Group::try_from(group)?)
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

#[cfg(feature = "rocksdb")]
type Db = forest_db::rocks::RocksDb;

#[cfg(feature = "paritydb")]
type Db = forest_db::parity_db::ParityDb;

fn main() {
    // Capture Cli inputs
    let Cli { opts, cmd } = Cli::from_args();

    // Run forest as a daemon if no other subcommands are used. Otherwise, run the subcommand.
    match opts.to_config() {
        Ok((cfg, path)) => {
            logger::setup_logger(&cfg.log, opts.color.into());
            ProgressBar::set_progress_bars_visibility(cfg.client.show_progress_bars);

            if let Some(path) = &path {
                match path {
                    ConfigPath::Env(path) => {
                        info!("FOREST_CONFIG_PATH loaded: {}", path.display())
                    }
                    ConfigPath::Project(path) => {
                        info!("Project config loaded: {}", path.display())
                    }
                    _ => (),
                }
                check_for_unknown_keys(path.to_path_buf(), &cfg);
            } else {
                info!("Using default {} config", cfg.chain.name);
            }
            match cmd {
                Some(_) => {
                    warn!("All subcommands have been moved to forest-cli tool");
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
                                cli_error_and_die(
                                    format!("Error building daemon. Error was: {e}"),
                                    1,
                                )
                            })
                            .start();
                        match result {
                            Ok(_) => info!("Process detached"),
                            Err(e) => {
                                cli_error_and_die(
                                    format!("Error when detaching. Error was: {e}"),
                                    1,
                                );
                            }
                        }
                    }

                    let rt = Runtime::new().unwrap();
                    if opts.tokio_console {
                        console_subscriber::init();
                    }

                    let db: Db = rt.block_on(daemon::start(cfg, opts.detach));

                    info!("Shutting down tokio...");
                    rt.shutdown_timeout(Duration::from_secs(10));

                    if let Err(e) = db.flush() {
                        error!("Error flushing db: {e}");
                    }
                    let db_weak_ref = Arc::downgrade(&db.db);
                    drop(db);

                    if db_weak_ref.strong_count() != 0 {
                        error!(
                            "Dangling reference to DB detected: {}. Tracking issue: https://github.com/ChainSafe/forest/issues/1891",
                            db_weak_ref.strong_count()
                        );
                    }
                    info!("Forest finish shutdown");
                }
            }
        }
        Err(e) => {
            logger::setup_logger(&LogConfig::default(), opts.color.into());
            cli_error_and_die(format!("Error parsing config: {e}"), 1);
        }
    };
}
