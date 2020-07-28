// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::cli::Subcommand;

/// Process CLI subcommand
pub(super) async fn process(command: Subcommand) {
    match command {
        Subcommand::Fetch(cmd) => {
            // TODO should pass in config?
            cmd.run().await;
        }
        Subcommand::Chain(cmd) => {
            // TODO should pass in config?
            cmd.run().await;
        }
    }
}
