// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use crate::cli::subcommands::Cli as ForestCli;
use crate::daemon::main::Cli as ForestDaemonCli;
use crate::tool::subcommands::Cli as ForestToolCli;
use crate::wallet::subcommands::Cli as ForestWalletCli;
use ahash::HashMap;
use clap::{Command, CommandFactory};
use clap_complete::aot::{Shell, generate};
use itertools::Itertools as _;

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
    pub fn run<W: std::io::Write>(self, writer: &mut W) -> anyhow::Result<()> {
        let mut bin_cmd_map: HashMap<String, Command> = HashMap::from_iter([
            ("forest".to_string(), ForestDaemonCli::command()),
            ("forest-cli".to_string(), ForestCli::command()),
            ("forest-wallet".to_string(), ForestWalletCli::command()),
            ("forest-tool".to_string(), ForestToolCli::command()),
        ]);

        let valid_binaries = bin_cmd_map.keys().cloned().collect_vec();
        let binaries = self.binaries.unwrap_or_else(|| valid_binaries.clone());

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
                writer,
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion_no_binaries_succeeds() {
        let cmd = CompletionCommand {
            binaries: None,
            shell: Shell::Bash,
        };

        // Execution should succeed
        let result = cmd.run(&mut std::io::sink());
        assert!(
            result.is_ok(),
            "Expected command to succeed, got: {result:?}"
        );
    }

    #[test]
    fn test_completion_binaries_succeeds() {
        let cmd = CompletionCommand {
            binaries: Some(vec!["forest-cli".to_string(), "forest-tool".to_string()]),
            shell: Shell::Bash,
        };

        let result = cmd.run(&mut std::io::sink());
        assert!(
            result.is_ok(),
            "Expected command to succeed, got {result:?}"
        );
    }

    #[test]
    fn test_completion_binaries_fails() {
        let cmd = CompletionCommand {
            binaries: Some(vec!["non-existent-binary".to_string()]),
            shell: Shell::Bash,
        };

        let result = cmd.run(&mut std::io::sink());
        assert!(
            result.is_err(),
            "Expected command to fail, but it succeeded"
        );

        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Unknown binary") && err.contains("non-existent-binary"),
            "Error message '{err}' did not contain expected text"
        );
    }

    #[test]
    fn test_completion_mixed_valid_invalid_fails() {
        // Create a completion command with mix of valid and invalid binaries
        let cmd = CompletionCommand {
            binaries: Some(vec![
                "forest-cli".to_string(),
                "non-existent-binary".to_string(),
            ]),
            shell: Shell::Bash,
        };

        let result = cmd.run(&mut std::io::sink());
        assert!(
            result.is_err(),
            "Expected command to fail, but it succeeded"
        );

        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Unknown binary") && err.contains("non-existent-binary"),
            "Error message '{err}' did not contain expected text"
        );
    }
}
