// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod config;
mod genesis;

pub use self::config::Config;
pub(super) use self::genesis::initialize_genesis;

use async_std::task;
use clap::{App, Arg};
use std::cell::RefCell;
use std::io;
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use utils::{read_file_to_string, read_toml};

pub(super) fn cli() -> Result<Config, io::Error> {
    let app = App::new("Forest")
        .version("0.0.1")
        .author("ChainSafe Systems <info@chainsafe.io>")
        .about("Filecoin implementation in Rust.")
        /*
         * Flags
         */
        .arg(
            Arg::with_name("config")
                .long("config")
                .short("c")
                .takes_value(true)
                .help("A toml file containing relevant configurations."),
        )
        .arg(
            Arg::with_name("genesis")
                .long("genesis")
                .takes_value(true)
                .help("The genesis CAR file"),
        )
        .get_matches();

    let mut cfg = match app.value_of("config") {
        Some(config_file) => {
            // Read from config file
            let toml = read_file_to_string(config_file)?;
            // Parse and return the configuration file
            read_toml(&toml)?
        }
        None => Config::default(),
    };
    if let Some(genesis_file) = app.value_of("genesis") {
        cfg.genesis_file = Some(genesis_file.to_owned());
    }
    // TODO in future parse all flags and append to a configuraiton object
    // Retrun defaults

    Ok(cfg)
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
