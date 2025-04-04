// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::cli_shared::cli::{CliOpts, HELP_MESSAGE};
use crate::cli_shared::{
    cli::{ConfigPath, DaemonConfig, check_for_unknown_keys, cli_error_and_die},
    logger,
};
use crate::daemon::ipc_shmem_conf;
use crate::utils::version::FOREST_VERSION_STRING;
use anyhow::Context as _;
use clap::Parser;
use daemonize_me::{Daemon, Group, User};
use raw_sync_2::{
    Timeout,
    events::{Event, EventInit},
};
use std::ffi::OsString;
use std::{fs::File, process, time::Duration};
use tracing::info;

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

    daemon = daemon.setup_post_fork_parent_hook(|_parent_pid, child_pid| {
        let mut shmem = ipc_shmem_conf().open().expect("open must succeed");
        shmem.set_owner(true);
        let (event, _) =
            unsafe { Event::from_existing(shmem.as_ptr()).expect("from_existing must succeed") };
        let ret = event.wait(Timeout::Infinite);
        drop(shmem); // Delete the local link and the shared memory object.
        if ret.is_err() {
            cli_error_and_die(
                "Error unblocking process. Check the log file for details.",
                1,
            );
        }
        info!("Forest has been detached and runs in the background (PID: {child_pid}).");
        process::exit(0);
    });

    Ok(daemon)
}

/// CLI structure generated when interacting with Forest binary
#[derive(Parser)]
#[command(name = env!("CARGO_PKG_NAME"), bin_name = "forest", author = env!("CARGO_PKG_AUTHORS"), version = FOREST_VERSION_STRING.as_str(), about = env!("CARGO_PKG_DESCRIPTION")
)]
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

    let (background_tasks, _guards) = logger::setup_logger(&opts);

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
        info!("Using default {} config", cfg.chain());
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

            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()?;

            for task in background_tasks {
                rt.spawn(task);
            }

            let ret = rt.block_on(super::start_interruptable(opts, cfg));
            info!("Shutting down tokio...");
            rt.shutdown_timeout(Duration::from_secs_f32(0.5));
            info!("Forest finish shutdown");
            ret
        }
    }
}
