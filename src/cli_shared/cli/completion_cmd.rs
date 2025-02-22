// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use clap::Command;
use clap_complete::aot::{generate, Shell};

/// Completion Command
#[derive(Debug, clap::Args)]
pub struct CompletionCommand {
    /// The Shell type to generate completions
    #[arg(long, default_value = "bash")]
    shell_type: Shell,
}

impl CompletionCommand {
    pub fn run(self, cmd: &mut Command) -> anyhow::Result<()> {
        let Some(bin_name) = cmd.get_bin_name() else {
            anyhow::bail!("invalid binary name")
        };

        generate(
            self.shell_type,
            cmd,
            bin_name.to_string(),
            &mut std::io::stdout(),
        );
        Ok(())
    }
}
