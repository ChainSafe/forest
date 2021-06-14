// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::cli::Subcommand;

/// Process CLI subcommand
pub(super) async fn process(command: Subcommand) {
    match command {
        Subcommand::Fetch(cmd) => {
            cmd.run().await;
        }
        Subcommand::Chain(cmd) => {
            cmd.run().await;
        }
        Subcommand::Auth(cmd) => {
            cmd.run().await;
        }

        Subcommand::Genesis(cmd) => {
            cmd.run().await;
        }
        Subcommand::Wallet(cmd) => {
            cmd.run().await;
        }
    }
}
