// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod wallet_cmd;

use crate::cli_shared::cli::HELP_MESSAGE;
use crate::cli_shared::cli::*;
use crate::utils::version::FOREST_VERSION_STRING;
use clap::Parser;
use jsonrpc_v2::Error as JsonRpcError;

/// Command-line options for the `forest-wallet` binary
#[derive(Parser)]
#[command(name = env!("CARGO_PKG_NAME"), author = env!("CARGO_PKG_AUTHORS"), version = FOREST_VERSION_STRING.as_str(), about = env!("CARGO_PKG_DESCRIPTION"))]
#[command(help_template(HELP_MESSAGE))]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: wallet_cmd::WalletCommands,
}

/// Pretty-print a JSON-RPC error and exit
pub(super) fn handle_rpc_err(e: JsonRpcError) -> anyhow::Error {
    match serde_json::to_string(&e) {
        Ok(err_msg) => anyhow::Error::msg(err_msg),
        Err(err) => err.into(),
    }
}
