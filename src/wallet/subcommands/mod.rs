// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod wallet_cmd;

use crate::cli_shared::cli::{CliRpcOpts, HELP_MESSAGE};
use crate::utils::version::FOREST_VERSION_STRING;
use clap::Parser;

/// Command-line options for the `forest-wallet` binary
#[derive(Parser)]
#[command(name = env!("CARGO_PKG_NAME"), bin_name = "forest-wallet", author = env!("CARGO_PKG_AUTHORS"), version = FOREST_VERSION_STRING.as_str(), about = env!("CARGO_PKG_DESCRIPTION")
)]
#[command(help_template(HELP_MESSAGE))]
pub struct Cli {
    #[clap(flatten)]
    pub opts: CliRpcOpts,

    /// Use remote wallet associated with the Filecoin node.
    /// Warning! You should ensure that your connection is encrypted and secure,
    /// as the communication between the wallet and the node is **not** encrypted.
    #[arg(long)]
    pub remote_wallet: bool,

    /// Encrypt local wallet
    #[arg(long)]
    pub encrypt: bool,

    #[command(subcommand)]
    pub cmd: wallet_cmd::WalletCommands,
}
