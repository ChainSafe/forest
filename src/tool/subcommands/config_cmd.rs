// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::io::Write;

use anyhow::Context;
use clap::Subcommand;

use super::read_config;

#[derive(Debug, Subcommand)]
pub enum ConfigCommands {
    /// Dump current configuration to standard output
    Dump {
        /// Optional TOML file containing forest daemon configuration
        #[arg(short, long)]
        config: Option<String>,
    },
}

impl ConfigCommands {
    pub fn run<W: Write + Unpin>(&self, sink: &mut W) -> anyhow::Result<()> {
        match self {
            Self::Dump { config } => {
                let config = read_config(config)?;
                writeln!(
                    sink,
                    "{}",
                    toml::to_string(&config)
                        .context("Could not convert configuration to TOML format")?
                )
                .context("Failed to write the configuration")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::subcommands::Config;

    #[tokio::test]
    async fn given_default_configuration_should_print_valid_toml() {
        let expected_config = Config::default();
        let mut sink = std::io::BufWriter::new(Vec::new());

        ConfigCommands::Dump { config: None }
            .run(&mut sink)
            .unwrap();

        let actual_config: Config = toml::from_str(std::str::from_utf8(sink.buffer()).unwrap())
            .expect("Invalid configuration!");

        assert!(expected_config == actual_config);
    }
}
