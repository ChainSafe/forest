// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::ffi::OsString;

use super::subcommands::Cli;
use crate::networks::NetworkChain;
use crate::rpc::{self, prelude::*};
use crate::shim::address::{CurrentNetwork, Network};
use clap::Parser;
use std::str::FromStr;

pub async fn main<ArgT>(args: impl IntoIterator<Item = ArgT>) -> anyhow::Result<()>
where
    ArgT: Into<OsString> + Clone,
{
    // Preliminary client without a token, used only to detect the target
    // network. Must happen BEFORE `Cli::parse_from` so that clap-driven
    // `StrictAddress` validation accepts testnet (`t0...`) addresses. Client
    // construction errors propagate (mirroring `forest-cli` in #6011); if the
    // daemon itself is unreachable, the global `CurrentNetwork` stays at its
    // mainnet default and testnet addresses will be rejected at parse time.
    let client = rpc::Client::default_or_from_env(None)?;
    if let Ok(name) = StateNetworkName::call(&client, ()).await
        && !matches!(NetworkChain::from_str(&name), Ok(NetworkChain::Mainnet))
    {
        CurrentNetwork::set_global(Network::Testnet);
    }

    // Capture Cli inputs
    let Cli {
        opts,
        remote_wallet,
        encrypt,
        cmd,
    } = Cli::parse_from(args);

    let client = rpc::Client::default_or_from_env(opts.token.as_deref())?;

    // Run command
    cmd.run(client, remote_wallet, encrypt).await
}
