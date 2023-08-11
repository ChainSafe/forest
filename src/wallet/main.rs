// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::ffi::OsString;

use super::subcommands::Cli;
use crate::cli::subcommands::cli_error_and_die;
use crate::networks::ChainConfig;
use crate::rpc_client::chain_get_name;
use crate::shim::address::{CurrentNetwork, Network};
use clap::Parser;
use std::sync::Arc;

pub fn main<ArgT>(args: impl IntoIterator<Item = ArgT>) -> anyhow::Result<()>
where
    ArgT: Into<OsString> + Clone,
{
    // Capture Cli inputs
    // TODO: Only keep the minimal flags for RPC calls (so --token, --rpc-address?)
    let Cli { opts, cmd } = Cli::parse_from(args);

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(async {
            match opts.to_config() {
                Ok((mut config, _)) => {
                    if opts.chain.is_none() {
                        if let Ok(name) = chain_get_name((), &config.client.rpc_token).await {
                            if name == "calibnet" {
                                config.chain = Arc::new(ChainConfig::calibnet());
                            } else if name == "devnet" {
                                config.chain = Arc::new(ChainConfig::devnet());
                            }
                        }
                    }
                    if config.chain.is_testnet() {
                        CurrentNetwork::set_global(Network::Testnet);
                    }
                    // Run command
                    cmd.run(&config).await
                }
                Err(e) => {
                    cli_error_and_die(format!("Error parsing config: {e}"), 1);
                }
            }
        })
}
