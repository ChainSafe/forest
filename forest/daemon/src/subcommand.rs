// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::cli::Subcommand;
use forest_cli_shared::cli::Config;

/// Process CLI sub-command
pub(super) async fn process(command: Subcommand, config: Config) {
    // Run command
    match command {
        Subcommand::Config(cmd) => {
            cmd.run(&config, &mut std::io::stdout()).await;
        }
    }
}
