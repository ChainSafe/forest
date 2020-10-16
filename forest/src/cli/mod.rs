// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod auth_cmd;
mod chain_cmd;
mod config;
mod fetch_params_cmd;
mod genesis_cmd;

pub(super) use self::auth_cmd::AuthCommands;
pub(super) use self::chain_cmd::ChainCommands;
pub use self::config::Config;
pub(super) use self::fetch_params_cmd::FetchCommands;
pub(super) use self::genesis_cmd::GenesisCommands;

use jsonrpc_v2::Error as JsonRpcError;
use std::cell::RefCell;
use std::io;
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use structopt::StructOpt;
use utils::{read_file_to_string, read_toml};

/// CLI structure generated when interacting with Forest binary
#[derive(StructOpt)]
#[structopt(
    name = "forest",
    version = "0.0.1",
    about = "Filecoin implementation in Rust. This command will start the daemon process",
    author = "ChainSafe Systems <info@chainsafe.io>"
)]
pub struct CLI {
    #[structopt(flatten)]
    pub daemon_opts: DaemonOpts,
    #[structopt(subcommand)]
    pub cmd: Option<Subcommand>,
}

/// Forest binary subcommands available.
#[derive(StructOpt)]
#[structopt(setting = structopt::clap::AppSettings::VersionlessSubcommands)]
pub enum Subcommand {
    #[structopt(
        name = "fetch-params",
        about = "Download parameters for generating and verifying proofs for given size"
    )]
    Fetch(FetchCommands),

    #[structopt(name = "chain", about = "Interact with Filecoin blockchain")]
    Chain(ChainCommands),

    #[structopt(name = "auth", about = "Manage RPC Permissions")]
    Auth(AuthCommands),

    #[structopt(name = "genesis", about = "Work with blockchain genesis")]
    Genesis(GenesisCommands),
}

/// Daemon process command line options.
#[derive(StructOpt, Debug)]
pub struct DaemonOpts {
    #[structopt(short, long, help = "A toml file containing relevant configurations")]
    pub config: Option<String>,
    #[structopt(short, long, help = "The genesis CAR file")]
    pub genesis: Option<String>,
    #[structopt(short, long, help = "Allow rpc to be active or not (default = true)")]
    pub rpc: Option<bool>,
    #[structopt(short, long, help = "The port used for communication")]
    pub port: Option<String>,
    #[structopt(short, long, help = "Allow Kademlia (default = true)")]
    pub kademlia: Option<bool>,
    #[structopt(short, long, help = "Allow MDNS (default = true)")]
    pub mdns: Option<bool>,
}

impl DaemonOpts {
    pub fn to_config(&self) -> Result<Config, io::Error> {
        let mut cfg: Config = match &self.config {
            Some(config_file) => {
                // Read from config file
                let toml = read_file_to_string(&*config_file)?;
                // Parse and return the configuration file
                read_toml(&toml)?
            }
            None => Config::default(),
        };
        if let Some(genesis_file) = &self.genesis {
            cfg.genesis_file = Some(genesis_file.to_owned());
        }
        if self.rpc.unwrap_or(cfg.enable_rpc) {
            cfg.enable_rpc = true;
            cfg.rpc_port = self.port.to_owned().unwrap_or(cfg.rpc_port);
        } else {
            cfg.enable_rpc = false;
        }

        cfg.network.kademlia = self.kademlia.unwrap_or(cfg.network.kademlia);
        cfg.network.mdns = self.mdns.unwrap_or(cfg.network.mdns);

        // (where to find these flags, should be easy to do with structops)

        Ok(cfg)
    }
}

/// Blocks current thread until ctrl-c is received
pub(super) async fn block_until_sigint() {
    let (ctrlc_send, ctrlc_oneshot) = futures::channel::oneshot::channel();
    let ctrlc_send_c = RefCell::new(Some(ctrlc_send));

    let running = Arc::new(AtomicUsize::new(0));
    ctrlc::set_handler(move || {
        let prev = running.fetch_add(1, Ordering::SeqCst);
        if prev == 0 {
            println!("Got interrupt, shutting down...");
            // Send sig int in channel to blocking task
            if let Some(ctrlc_send) = ctrlc_send_c.try_borrow_mut().unwrap().take() {
                ctrlc_send.send(()).expect("Error sending ctrl-c message");
            }
        } else {
            process::exit(0);
        }
    })
    .expect("Error setting Ctrl-C handler");

    ctrlc_oneshot.await.unwrap();
}

/// Returns a stringified JSON-RPC error
pub(super) fn stringify_rpc_err(e: JsonRpcError) -> String {
    match e {
        JsonRpcError::Full {
            code,
            message,
            data: _,
        } => {
            return format!("JSON RPC Error: Code: {} Message: {}", code, message);
        }
        JsonRpcError::Provided { code, message } => {
            return format!("JSON RPC Error: Code: {} Message: {}", code, message);
        }
    }
}
