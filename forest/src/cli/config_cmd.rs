// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::cli::Config;
use async_std::io::Write;
use async_std::io::WriteExt;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
pub enum ConfigCommands {
    #[structopt(about = "Dump current configuration to standard output")]
    Dump,
}

impl ConfigCommands {
    pub async fn run<W: Write + Unpin>(&self, config: &Config, sink: &mut W) {
        match self {
            Self::Dump => {
                writeln!(
                    sink,
                    "{}",
                    toml::to_string(config)
                        .expect("Could not convert configuration to TOML format")
                )
                .await
                .expect("Failed to write the configuration");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[async_std::test]
    async fn given_default_configuration_should_print_valid_toml() {
        let expected_config = Config::default();
        let mut sink = futures::io::BufWriter::new(Vec::new());

        ConfigCommands::Dump.run(&expected_config, &mut sink).await;

        let actual_config: Config = toml::from_str(std::str::from_utf8(sink.buffer()).unwrap())
            .expect("Invalid configuration!");

        assert!(expected_config == actual_config);
    }
}
