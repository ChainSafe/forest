// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use structopt::StructOpt;

#[allow(missing_docs)]
#[derive(Debug, StructOpt)]
pub struct WalletCommands {
    #[structopt(long, help = "Generate a new key of the given typ")]
    pub new: bool,
}
