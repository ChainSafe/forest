// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

// Due to https://git.wiki.kernel.org/index.php/GitFaq#Why_does_Git_not_.22track.22_renames.3F
// we cannot rewire the git history of this file.
// check out the original commit history here:
// https://github.com/ChainSafe/forest/commits/main/forest/src/cli/mod.rs

mod auth_cmd;
mod chain_cmd;
mod config_cmd;
mod f3_cmd;
mod healthcheck_cmd;
mod info_cmd;
mod mpool_cmd;
mod net_cmd;
pub(crate) mod send_cmd;
mod shutdown_cmd;
mod snapshot_cmd;
mod state_cmd;
mod sync_cmd;

use std::io::Write;

pub(crate) use crate::cli_shared::cli::Config;
use crate::cli_shared::cli::HELP_MESSAGE;
use crate::utils::version::FOREST_VERSION_STRING;
use crate::{lotus_json::HasLotusJson};
use clap::Parser;
use tracing::error;

pub(super) use self::{
    auth_cmd::AuthCommands, chain_cmd::ChainCommands, config_cmd::ConfigCommands,
    f3_cmd::F3Commands, healthcheck_cmd::HealthcheckCommand, mpool_cmd::MpoolCommands,
    net_cmd::NetCommands, send_cmd::SendCommand, shutdown_cmd::ShutdownCommand,
    snapshot_cmd::SnapshotCommands, state_cmd::StateCommands, sync_cmd::SyncCommands,
};
use crate::cli::subcommands::info_cmd::InfoCommand;

/// CLI structure generated when interacting with Forest binary
#[derive(Parser)]
#[command(name = env!("CARGO_PKG_NAME"), author = env!("CARGO_PKG_AUTHORS"), version = FOREST_VERSION_STRING.as_str(), about = env!("CARGO_PKG_DESCRIPTION"))]
#[command(help_template(HELP_MESSAGE))]
pub struct Cli {
    /// Client JWT token to use for JSON-RPC authentication
    #[arg(short, long)]
    pub token: Option<String>,
    #[command(subcommand)]
    pub cmd: Subcommand,
}

/// Forest binary sub-commands available.
#[derive(clap::Subcommand, Debug)]
pub enum Subcommand {
    /// Interact with Filecoin blockchain
    #[command(subcommand)]
    Chain(ChainCommands),

    /// Manage RPC permissions
    #[command(subcommand)]
    Auth(AuthCommands),

    /// Manage P2P network
    #[command(subcommand)]
    Net(NetCommands),

    /// Inspect or interact with the chain synchronizer
    #[command(subcommand)]
    Sync(SyncCommands),

    /// Interact with the message pool
    #[command(subcommand)]
    Mpool(MpoolCommands),

    /// Interact with and query Filecoin chain state
    #[command(subcommand)]
    State(StateCommands),

    /// Manage node configuration
    #[command(subcommand)]
    Config(ConfigCommands),

    /// Manage snapshots
    #[command(subcommand)]
    Snapshot(SnapshotCommands),

    /// Send funds between accounts
    Send(SendCommand),

    /// Print node info
    #[command(subcommand)]
    Info(InfoCommand),

    /// `[REMOVED]` Attach to daemon via a JavaScript console
    Attach { _ignored: Vec<String> },

    /// Shutdown Forest
    Shutdown(ShutdownCommand),

    /// Print healthcheck info
    #[command(subcommand)]
    Healthcheck(HealthcheckCommand),

    /// Manages Filecoin Fast Finality (F3) interactions
    #[command(subcommand)]
    F3(F3Commands),
}

/// Format a vector to a prettified string
pub(super) fn format_vec_pretty(vec: Vec<String>) -> String {
    format!("[{}]", vec.join(", "))
}

/// Print an error message and exit the program with an error code
/// Used for handling high level errors such as invalid parameters
pub fn cli_error_and_die(msg: impl AsRef<str>, code: i32) -> ! {
    error!("Error: {}", msg.as_ref());
    std::process::exit(code);
}

/// Prints a pretty HTTP JSON-RPC response result
pub(super) fn print_pretty_lotus_json<T: HasLotusJson>(obj: T) -> anyhow::Result<()> {
    println!("{}", obj.into_lotus_json_string_pretty()?);
    Ok(())
}

/// Prints a bytes HTTP JSON-RPC response result
pub(super) fn print_rpc_res_bytes(obj: Vec<u8>) -> anyhow::Result<()> {
    println!("{}", String::from_utf8(obj)?);
    Ok(())
}

/// Require user confirmation. Returns `false` when not connected to a terminal.
pub fn prompt_confirm() -> bool {
    let term = dialoguer::console::Term::stderr();

    if !term.is_term() {
        return false;
    }

    dialoguer::Confirm::new()
        .with_prompt("Do you want to continue?")
        .interact_on(&term)
        .unwrap_or(false)
}
