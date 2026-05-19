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
    use clap::error::ErrorKind;
    use rstest::rstest;

    fn try_parse(args: &[&str]) -> Result<Cli, ErrorKind> {
        let argv = std::iter::once("forest-wallet").chain(args.iter().copied());
        Cli::try_parse_from(argv).map_err(|e| e.kind())
    }

    #[rstest]
    #[case::balance(&["balance", "not-an-address"])]
    #[case::export(&["export", "not-an-address"])]
    #[case::has(&["has", "not-an-address"])]
    #[case::set_default(&["set-default", "not-an-address"])]
    #[case::delete(&["delete", "not-an-address"])]
    #[case::sign(&["sign", "-m", "de", "-a", "not-an-address"])]
    #[case::verify(&["verify", "-a", "not-an-address", "-m", "de", "-s", "de"])]
    #[case::send_from(&["send", "--from", "not-an-address", "f01234", "1"])]
    fn migrated_subcommands_reject_malformed_address(#[case] args: &[&str]) {
        assert_eq!(try_parse(args).err(), Some(ErrorKind::ValueValidation));
    }
}
