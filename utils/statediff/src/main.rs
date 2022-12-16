// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use directories::ProjectDirs;
use std::path::Path;
use structopt::StructOpt;

use cid::Cid;
use forest_statediff::print_state_diff;

/// Examine the state delta
#[derive(StructOpt)]
pub struct ChainCommand {
    /// The previous CID state root
    pre: Cid,
    /// The post CID state root
    post: Cid,
    /// The name of the chain
    #[structopt(short, long, default_value = "mainnet")]
    chain: String,
    /// The depth at which IPLD links are resolved
    #[structopt(short, long)]
    depth: Option<u64>,
}

impl ChainCommand {
    pub async fn run(&self) {
        let dir = ProjectDirs::from("com", "ChainSafe", "Forest").unwrap();
        let chain_path = dir.data_dir().join(&self.chain);
        let blockstore = open_db(&chain_path);

        if let Err(err) = print_state_diff(&blockstore, &self.pre, &self.post, self.depth) {
            eprintln!("Failed to print state diff: {err}");
        }
    }
}

#[cfg(feature = "rocksdb")]
fn open_db(chain_path: &Path) -> forest_db::rocks::RocksDb {
    forest_db::rocks::RocksDb::open(
        chain_path.join("rocksdb"),
        &forest_db::rocks_config::RocksDbConfig::default(),
    )
    .expect("Opening RocksDB must succeed")
}

#[cfg(feature = "paritydb")]
fn open_db(chain_path: &Path) -> forest_db::parity_db::ParityDb {
    use forest_db::parity_db::*;
    use forest_db::parity_db_config::*;
    ParityDb::open(chain_path.join("paritydb"), &ParityDbConfig::default())
        .expect("Opening ParityDb must succeed")
}

/// statediff binary sub-commands available.
#[derive(StructOpt)]
#[structopt(setting = structopt::clap::AppSettings::VersionlessSubcommands)]
enum Subcommand {
    #[structopt(name = "chain")]
    Chain(ChainCommand),
}

/// CLI structure generated when interacting with the statediff tool
#[derive(StructOpt)]
#[structopt(
    name = env!("CARGO_PKG_NAME"),
    version = env!("CARGO_PKG_VERSION"),
    about = env!("CARGO_PKG_DESCRIPTION"),
    author = env!("CARGO_PKG_AUTHORS")
)]
struct Cli {
    #[structopt(subcommand)]
    cmd: Subcommand,
}

#[tokio::main]
async fn main() {
    // Capture Cli inputs
    let Cli { cmd } = Cli::from_args();
    match cmd {
        Subcommand::Chain(cmd) => {
            cmd.run().await;
        }
    }
}
