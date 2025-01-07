// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::ffi::OsString;

use super::subcommands::Cli;
use crate::networks::NetworkChain;
use crate::rpc::{self, prelude::*};
use crate::shim::address::{CurrentNetwork, Network};
use clap::Parser;
use std::str::FromStr;

pub fn main<ArgT>(args: impl IntoIterator<Item = ArgT>) -> anyhow::Result<()>
where
    ArgT: Into<OsString> + Clone,
{
    // Capture Cli inputs
    let Cli {
        opts,
        remote_wallet,
        encrypt,
        cmd,
    } = Cli::parse_from(args);

    let client = rpc::Client::default_or_from_env(opts.token.as_deref())?;

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let name = StateNetworkName::call(&client, ()).await?;
            let chain = NetworkChain::from_str(&name)?;
            if chain.is_testnet() {
                CurrentNetwork::set_global(Network::Testnet);
            }
            // Run command
            cmd.run(client, remote_wallet, encrypt).await
        })
}
