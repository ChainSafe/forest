// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::cli::subcommands::Cli as ForestCliSubCommand;
use crate::tool::subcommands::Cli as ForestToolSubCommand;
use crate::wallet::subcommands::Cli as ForestWalletSubCommand;
use crate::daemon::main::Cli as ForestDaemonSubCommand;
use ahash::HashMap;
use clap::{Command, CommandFactory};
use clap_complete::aot::{generate, Shell};

/// Completion Command for generating shell completions for the CLI
#[derive(Debug, clap::Args)]
pub struct CompletionCommand {
    /// The binaries for which to generate completions (e.g., 'forest-cli,forest-tool,forest-wallet').
    /// If omitted, completions for all known binaries will be generated.
    #[arg(value_delimiter = ',')]
    binaries: Option<Vec<String>>,
    /// The Shell type to generate completions for
    #[arg(long, default_value = "bash")]
    shell: Shell,
}

impl CompletionCommand {
    pub fn run(self) -> anyhow::Result<()> {
        let mut bin_cmd_map: HashMap<String, Command> = HashMap::from_iter([
            ("forest".to_string(), ForestDaemonSubCommand::command()),
            ("forest-cli".to_string(), ForestCliSubCommand::command()),
            ("forest-wallet".to_string(), ForestWalletSubCommand::command()),
            ("forest-tool".to_string(), ForestToolSubCommand::command()),
        ]);

        let valid_binaries = bin_cmd_map.keys().cloned().collect::<Vec<_>>();
        let binaries = self
            .binaries
            .unwrap_or_else(|| valid_binaries.clone());

        for b in binaries {
            let cmd = bin_cmd_map.get_mut(&b).ok_or_else(|| {
                anyhow::anyhow!(
                    "Unknown binary: '{}'. Valid binaries are: {:?}",
                    b,
                    valid_binaries.join(",")
                )
            })?;

            generate(
                self.shell,
                cmd,
                cmd.get_bin_name().unwrap().to_string(),
                &mut std::io::stdout(),
            );
        }
        Ok(())
    }
}
