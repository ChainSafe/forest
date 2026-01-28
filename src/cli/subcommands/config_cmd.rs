// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::io::Write;

use anyhow::Context as _;
use clap::Subcommand;

use crate::{cli::subcommands::Config, rpc};

#[derive(Debug, Subcommand)]
pub enum ConfigCommands {
    /// Dump default configuration to standard output
    Dump,
}

impl ConfigCommands {
    pub async fn run(self, _: rpc::Client) -> anyhow::Result<()> {
        self.run_internal(&mut std::io::stdout())
    }

    fn run_internal<W: Write + Unpin>(self, sink: &mut W) -> anyhow::Result<()> {
        match self {
            Self::Dump => writeln!(
                sink,
                "{}",
                toml::to_string(&Config::default())
                    .context("Could not convert configuration to TOML format")?
            )
            .context("Failed to write the configuration"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn given_default_configuration_should_print_valid_toml() {
        let expected_config = Config::default();
        let mut sink = std::io::BufWriter::new(Vec::new());

        ConfigCommands::Dump.run_internal(&mut sink).unwrap();

        let actual_config: Config = toml::from_str(std::str::from_utf8(sink.buffer()).unwrap())
            .expect("Invalid configuration!");

        assert_eq!(expected_config, actual_config);
    }
}
