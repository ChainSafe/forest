// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::io::Write;

use anyhow::Context;
use clap::Subcommand;

use crate::cli::subcommands::Config;

#[derive(Debug, Subcommand)]
pub enum ConfigCommands {
    /// Dump current configuration to standard output
    Dump,
}

impl ConfigCommands {
    pub fn run<W: Write + Unpin>(&self, config: &Config, sink: &mut W) -> anyhow::Result<()> {
        match self {
            Self::Dump => writeln!(
                sink,
                "{}",
                toml::to_string(config)
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

        ConfigCommands::Dump
            .run(&expected_config, &mut sink)
            .unwrap();

        let actual_config: Config = toml::from_str(std::str::from_utf8(sink.buffer()).unwrap())
            .expect("Invalid configuration!");

        assert!(expected_config == actual_config);
    }
}
