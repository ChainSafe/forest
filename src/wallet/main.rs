// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::ffi::OsString;

use super::subcommands::{handle_rpc_err, Cli};
use crate::networks::NetworkChain;
use crate::rpc_client::ApiInfo;
use crate::shim::address::{CurrentNetwork, Network};
use clap::Parser;
use std::str::FromStr;

pub fn main<ArgT>(args: impl IntoIterator<Item = ArgT>) -> anyhow::Result<()>
where
    ArgT: Into<OsString> + Clone,
{
    // Capture Cli inputs
    let Cli { opts, cmd } = Cli::parse_from(args);

    let api = ApiInfo::from_env()?.set_token(opts.token.clone());

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let name = api.state_network_name().await.map_err(handle_rpc_err)?;
            let chain = NetworkChain::from_str(&name)?;
            if chain.is_testnet() {
                CurrentNetwork::set_global(Network::Testnet);
            }
            // Run command
            cmd.run(opts.token).await
        })
}
