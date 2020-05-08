// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod config;
mod genesis;

pub use self::config::Config;
pub(super) use self::genesis::initialize_genesis;

use async_std::task;
use std::cell::RefCell;
use std::io;
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use structopt::StructOpt;
use utils::{read_file_to_string, read_toml};

#[derive(Debug, StructOpt)]
#[structopt(
    name = "Forest",
    version = "0.0.1",
    about = "Filecoin implementation in Rust",
    author = "ChainSafe Systems <info@chainsafe.io>"
)]
pub struct CLI {
    #[structopt(short, long, help = "A toml file containing relevant configurations.")]
    pub config: Option<String>,
    #[structopt(short, long, help = "The genesis CAR file")]
    pub genesis: Option<String>,
}

impl CLI {
    pub fn get_config(&self) -> Result<Config, io::Error> {
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
        // (where to find these flags, should be easy to do with structops)

        Ok(cfg)
    }
}

/// Blocks current thread until ctrl-c is received
pub(super) fn block_until_sigint() {
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

    task::block_on(ctrlc_oneshot).unwrap();
}
