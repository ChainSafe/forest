// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

#[cfg(feature = "jemalloc")]
use forest_cli_shared::tikv_jemallocator::Jemalloc;
#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[cfg(feature = "mimalloc")]
use forest_cli_shared::mimalloc::MiMalloc;
#[cfg(feature = "mimalloc")]
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

mod cli;
mod subcommand;

use clap::Parser;
use cli::{cli_error_and_die, Cli};
use forest_cli_shared::{cli::LogConfig, logger};
use forest_utils::io::ProgressBar;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Capture Cli inputs
    let Cli { opts, cmd } = Cli::parse();

    match opts.to_config() {
        Ok((cfg, _)) => {
            logger::setup_logger(&cfg.log, &opts);
            ProgressBar::set_progress_bars_visibility(cfg.client.show_progress_bars);
            subcommand::process(cmd, cfg).await
        }
        Err(e) => {
            logger::setup_logger(&LogConfig::default(), &opts);
            cli_error_and_die(format!("Error parsing config: {e}"), 1);
        }
    }
}
