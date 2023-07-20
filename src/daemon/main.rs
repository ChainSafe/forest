// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::cli_shared::cli::{CliOpts, HELP_MESSAGE};
use crate::cli_shared::{
    cli::{check_for_unknown_keys, cli_error_and_die, ConfigPath, DaemonConfig},
    logger,
};
use crate::daemon::ipc_shmem_conf;
use crate::utils::io::ProgressBar;
use crate::utils::version::FOREST_VERSION_STRING;
use anyhow::Context;
use clap::Parser;
use daemonize_me::{Daemon, Group, User};
use raw_sync::{
    events::{Event, EventInit},
    Timeout,
};
use std::ffi::OsString;
use std::{cmp::max, fs::File, process, time::Duration};
use tokio::runtime::Builder as RuntimeBuilder;
use tracing::info;

const EVENT_TIMEOUT: Timeout = Timeout::Val(Duration::from_secs(20));

// Initiate an Event object in shared memory.
fn create_ipc_lock() -> anyhow::Result<()> {
    let mut shmem = ipc_shmem_conf().create()?;
    // The shared memory object will not be deleted when 'shmem' is dropped
    // because we're not the owner.
    shmem.set_owner(false);
    unsafe { Event::new(shmem.as_ptr(), true).map_err(|err| anyhow::anyhow!("{err}")) }?;
    Ok(())
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

/// CLI structure generated when interacting with Forest binary
#[derive(Parser)]
#[command(name = env!("CARGO_PKG_NAME"), author = env!("CARGO_PKG_AUTHORS"), version = FOREST_VERSION_STRING.as_str(), about = env!("CARGO_PKG_DESCRIPTION"))]
#[command(help_template(HELP_MESSAGE))]
pub struct Cli {
    #[clap(flatten)]
    pub opts: CliOpts,
    pub cmd: Option<String>,
}

pub fn main<ArgT>(args: impl IntoIterator<Item = ArgT>) -> anyhow::Result<()>
where
    ArgT: Into<OsString> + Clone,
{
    // Capture Cli inputs
    let Cli { opts, cmd } = Cli::parse_from(args);

    let (cfg, path) = opts.to_config().context("Error parsing config")?;

    // Run forest as a daemon if no other subcommands are used. Otherwise, run the
    // subcommand.

    let (loki_task, _chrome_flush_guard) = logger::setup_logger(&opts);
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
        info!("Using default {} config", cfg.chain.network);
    }
    if opts.dry_run {
        return Ok(());
    }
    match cmd {
        Some(subcmd) => {
            anyhow::bail!(
                "Invalid subcommand: {subcmd}. All subcommands have been moved to forest-cli tool."
            );
        }
        None => {
            if opts.detach {
                create_ipc_lock()?;
                info!(
                    "Redirecting stdout and stderr to files {} and {}.",
                    cfg.daemon.stdout.display(),
                    cfg.daemon.stderr.display()
                );
                build_daemon(&cfg.daemon)?.start()?;
            }

            let mut builder = RuntimeBuilder::new_multi_thread();
            builder.enable_io().enable_time();

            if let Some(worker_threads) = cfg.tokio.worker_threads {
                builder.worker_threads(max(1, worker_threads));
            }
            if let Some(max_blocking_threads) = cfg.tokio.max_blocking_threads {
                builder.max_blocking_threads(max(1, max_blocking_threads));
            }
            if let Some(thread_keep_alive) = cfg.tokio.thread_keep_alive {
                builder.thread_keep_alive(thread_keep_alive);
            }
            if let Some(thread_stack_size) = cfg.tokio.thread_stack_size {
                builder.thread_stack_size(thread_stack_size);
            }
            if let Some(global_queue_interval) = cfg.tokio.global_queue_interval {
                builder.global_queue_interval(global_queue_interval);
            }

            let rt = builder.build()?;

            if let Some(loki_task) = loki_task {
                rt.spawn(loki_task);
            }
            let ret = rt.block_on(super::start_interruptable(opts, cfg));
            info!("Shutting down tokio...");
            rt.shutdown_timeout(Duration::from_secs_f32(0.5));
            info!("Forest finish shutdown");
            ret
        }
    }
}
