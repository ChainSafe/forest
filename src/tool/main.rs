// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::cli_shared::cli::*;

use std::ffi::OsString;

use super::subcommands::Cli;
use clap::Parser;

use crate::utils::{io::read_file_to_string, io::read_toml};

use super::subcommands::Subcommand;

fn read_config() -> anyhow::Result<Config> {
    let opts = CliOpts::default();
    let path = find_config_path(&opts);
    let cfg: Config = match &path {
        Some(path) => {
            // Read from config file
            let toml = read_file_to_string(path.to_path_buf())?;
            // Parse and return the configuration file
            read_toml(&toml)?
        }
        None => Config::default(),
    };
    Ok(cfg)
}

pub fn main<ArgT>(args: impl IntoIterator<Item = ArgT>) -> anyhow::Result<()>
where
    ArgT: Into<OsString> + Clone,
{
    // Capture Cli inputs
    let Cli { cmd } = Cli::parse_from(args);

    let config = read_config()?;

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            // Run command
            match cmd {
                Subcommand::Config(cmd) => cmd.run(&config, &mut std::io::stdout()),
                Subcommand::Benchmark(cmd) => cmd.run().await,
                Subcommand::DB(cmd) => cmd.run().await,
            }
        })
}
