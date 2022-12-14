// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod cli;
mod subcommand;

use cli::{cli_error_and_die, Cli};

use forest_cli_shared::{cli::LogConfig, logger};
use forest_utils::io::ProgressBar;
use structopt::StructOpt;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Capture Cli inputs
    let Cli { opts, cmd } = Cli::from_args();

    match opts.to_config() {
        Ok((cfg, _)) => {
            logger::setup_logger(&cfg.log, opts.color.into());
            ProgressBar::set_progress_bars_visibility(cfg.client.show_progress_bars);
            subcommand::process(cmd, cfg).await
        }
        Err(e) => {
            logger::setup_logger(&LogConfig::default(), opts.color.into());
            cli_error_and_die(format!("Error parsing config: {e}"), 1);
        }
    }
}
