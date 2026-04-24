// Copyright 2019-2026 ChainSafe Systems
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

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::Parser;

    /// Guards against an accidental revert of #6012 (address args ↔ `StrictAddress`).
    /// Malformed addresses must be rejected by clap at parse time, with a
    /// `ValueValidation` error rather than succeeding and failing later.
    fn parse_err_kind(args: &[&str]) -> clap::error::ErrorKind {
        match Cli::try_parse_from(args.iter().copied()) {
            Ok(_) => panic!("expected clap parse to fail for {args:?}"),
            Err(e) => e.kind(),
        }
    }

    #[test]
    fn wallet_balance_rejects_malformed_address() {
        assert_eq!(
            parse_err_kind(&["forest-wallet", "balance", "not-an-address"]),
            clap::error::ErrorKind::ValueValidation,
        );
    }

    #[test]
    fn wallet_sign_rejects_malformed_address() {
        assert_eq!(
            parse_err_kind(&[
                "forest-wallet",
                "sign",
                "-m",
                "deadbeef",
                "-a",
                "not-an-address",
            ]),
            clap::error::ErrorKind::ValueValidation,
        );
    }
}
