// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::cli::{Config, Subcommand};
use super::logger;

/// Process CLI sub-command
pub(super) async fn process(command: Subcommand, config: Config) {
    logger::setup_logger(config.log_file.clone());
    // Run command
    match command {
        Subcommand::Fetch(cmd) => {
            cmd.run(config).await;
        }
        Subcommand::Chain(cmd) => {
            cmd.run().await;
        }
        Subcommand::Auth(cmd) => {
            cmd.run(config).await;
        }
        Subcommand::Genesis(cmd) => {
            cmd.run().await;
        }
        Subcommand::Net(cmd) => {
            cmd.run().await;
        }
        Subcommand::Wallet(cmd) => {
            cmd.run().await;
        }
        Subcommand::Sync(cmd) => {
            cmd.run().await;
        }
        Subcommand::Mpool(cmd) => {
            cmd.run().await;
        }
        Subcommand::State(cmd) => {
            cmd.run().await;
        }
        Subcommand::Config(cmd) => {
            cmd.run(&config, &mut async_std::io::stdout()).await;
        }
    }
}
