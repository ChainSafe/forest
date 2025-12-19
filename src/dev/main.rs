// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::subcommands::Cli;
use crate::cli_shared::logger::setup_minimal_logger;
use clap::Parser as _;
use std::ffi::OsString;

pub async fn main<ArgT>(args: impl IntoIterator<Item = ArgT>) -> anyhow::Result<()>
where
    ArgT: Into<OsString> + Clone,
{
    // Capture Cli inputs
    let Cli { cmd } = Cli::parse_from(args);
    setup_minimal_logger();
    let client = crate::rpc::Client::default_or_from_env(None)?;
    cmd.run(client).await
}
