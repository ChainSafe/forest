// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::cli::subcommands::Cli;
use crate::cli_shared::logger;
use crate::networks::NetworkChain;
use crate::rpc::{self, prelude::*};
use crate::shim::address::{CurrentNetwork, Network};
use clap::Parser;
use std::ffi::OsString;
use std::str::FromStr as _;

pub async fn main<ArgT>(args: impl IntoIterator<Item = ArgT>) -> anyhow::Result<()>
where
    ArgT: Into<OsString> + Clone,
{
    // Preliminary client without the token to check network. This needs to occur before parsing to ensure the
    // `StrictAddress` validation works correctly.
    let client = rpc::Client::default_or_from_env(None)?;
    if let Ok(name) = StateNetworkName::call(&client, ()).await
        && !matches!(NetworkChain::from_str(&name), Ok(NetworkChain::Mainnet))
    {
        CurrentNetwork::set_global(Network::Testnet);
    }

    // Capture Cli inputs
    let Cli { token, cmd } = Cli::parse_from(args);

    let client = rpc::Client::default_or_from_env(token.as_deref())?;

    let (_bg_tasks, _guards) = logger::setup_logger(&crate::cli_shared::cli::CliOpts::default());

    cmd.run(client).await
}
