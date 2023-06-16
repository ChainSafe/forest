// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use cid::Cid;
use clap::Parser;
use directories::ProjectDirs;
use crate::cli_shared::cli::HELP_MESSAGE;
use crate::db::db_engine::{db_root, open_proxy_db};
use crate::statediff::print_state_diff;

impl crate::statediff::Subcommand {
    pub fn run(&self) -> anyhow::Result<()> {
        match self {
            Subcommand::ChainCommand {
                pre,
                post,
                chain,
                depth,
            } => {
                let dir = ProjectDirs::from("com", "ChainSafe", "Forest")
                    .ok_or(anyhow::Error::msg("no such path"))?;
                let chain_path = dir.data_dir().join(chain);
                let blockstore = open_proxy_db(db_root(&chain_path), Default::default())?;

                if let Err(err) = print_state_diff(&blockstore, pre, post, *depth) {
                    eprintln!("Failed to print state diff: {err}");
                }
                Ok(())
            }
        }
    }
}

/// CLI structure generated when interacting with the statediff tool
#[derive(Parser)]
#[command(name = env!("CARGO_PKG_NAME"), author = env!("CARGO_PKG_AUTHORS"), version = env!("CARGO_PKG_VERSION"), about = env!("CARGO_PKG_DESCRIPTION"))]
#[command(help_template(HELP_MESSAGE))]
struct Cli {
    #[command(subcommand)]
    cmd: Subcommand,
}

#[derive(clap::Subcommand)]
enum Subcommand {
    #[command(name = "chain")]
    ChainCommand {
        /// The previous CID state root
        pre: Cid,
        /// The post CID state root
        post: Cid,
        /// The name of the chain
        #[arg(short, long, default_value = "mainnet")]
        chain: String,
        /// The depth at which IPLD links are resolved
        #[arg(short, long)]
        depth: Option<u64>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Capture Cli inputs
    let Cli { cmd } = Cli::parse();
    cmd.run()?;
    Ok(())
}
