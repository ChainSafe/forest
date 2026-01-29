// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::cli_shared::cli::{CliOpts, HELP_MESSAGE};
use crate::cli_shared::{
    cli::{ConfigPath, check_for_unknown_keys},
    logger,
};
use crate::utils::version::FOREST_VERSION_STRING;
use anyhow::Context as _;
use clap::Parser;
use std::ffi::OsString;
use std::time::Duration;
use tracing::info;

/// CLI structure generated when interacting with Forest binary
#[derive(Parser)]
#[command(name = env!("CARGO_PKG_NAME"), bin_name = "forest", author = env!("CARGO_PKG_AUTHORS"), version = FOREST_VERSION_STRING.as_str(), about = env!("CARGO_PKG_DESCRIPTION")
)]
#[command(help_template(HELP_MESSAGE))]
pub struct Cli {
    #[clap(flatten)]
    pub opts: CliOpts,
}

pub fn main<ArgT>(args: impl IntoIterator<Item = ArgT>) -> anyhow::Result<()>
where
    ArgT: Into<OsString> + Clone,
{
    // Capture Cli inputs
    let Cli { opts } = Cli::parse_from(args);

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
