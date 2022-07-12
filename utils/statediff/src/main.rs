// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use directories::ProjectDirs;
use structopt::StructOpt;

use cid::Cid;
use db::rocks::RocksDb;
use db::rocks_config::RocksDbConfig;
use statediff::print_state_diff;

#[derive(StructOpt)]
pub struct ChainCommand {
    #[structopt(help = "The pre cid object")]
    pre: Cid,
    #[structopt(help = "The post cid object")]
    post: Cid,
    #[structopt(short, long, help = "The name of the chain", default_value = "mainnet")]
    chain: String,
    #[structopt(short, long, help = "The depth at which ipld links are resolved")]
    depth: Option<u64>,
}

impl ChainCommand {
    pub async fn run(&self) {
        let dir = ProjectDirs::from("com", "ChainSafe", "Forest").expect("failed to find project directories, please set FOREST_CONFIG_PATH environment variable manually.");
        let mut path = dir.data_dir().to_path_buf();
        path.push(&self.chain);
        path.push("db");

        let bs =
            RocksDb::open(path, &RocksDbConfig::default()).expect("Opening RocksDB must succeed");

        if let Err(err) = print_state_diff(&bs, &self.pre, &self.post, self.depth) {
            eprintln!("Failed to print state diff: {}", err);
        }
    }
}

/// statediff binary subcommands available.
#[derive(StructOpt)]
#[structopt(setting = structopt::clap::AppSettings::VersionlessSubcommands)]
#[structopt(about = "statediff subcommands")]
enum Subcommand {
    #[structopt(name = "chain", about = "Examine the state delta")]
    Chain(ChainCommand),
}

/// CLI structure generated when interacting with the statediff tool
#[derive(StructOpt)]
#[structopt(
    name = env!("CARGO_PKG_NAME"),
    version = option_env!("FOREST_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")),
    about = env!("CARGO_PKG_DESCRIPTION"),
    author = env!("CARGO_PKG_AUTHORS")
)]
struct Cli {
    #[structopt(subcommand)]
    cmd: Subcommand,
}

#[async_std::main]
async fn main() {
    // Capture Cli inputs
    let Cli { cmd } = Cli::from_args();
    match cmd {
        Subcommand::Chain(cmd) => {
            cmd.run().await;
        }
    }
}
