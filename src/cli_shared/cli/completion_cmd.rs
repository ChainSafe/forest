// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::cli::subcommands::Cli as CliSubCommand;
use crate::tool::subcommands::Cli as ToolSubCommand;
use crate::wallet::subcommands::Cli as WalletSubCommand;
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
    /// The Shell type to generate completions
    #[arg(long, default_value = "bash")]
    shell: Shell,
}

impl CompletionCommand {
    pub fn run(self) -> anyhow::Result<()> {
        let mut bin_cmd_map: HashMap<String, Command> = HashMap::from_iter([
            ("forest-cli".to_string(), CliSubCommand::command()),
            ("forest-wallet".to_string(), WalletSubCommand::command()),
            ("forest-tool".to_string(), ToolSubCommand::command()),
        ]);

        let binaries = self
            .binaries
            .unwrap_or_else(|| bin_cmd_map.keys().cloned().collect());

        for b in binaries {
            let cmd = bin_cmd_map.get_mut(&b).unwrap();
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
