// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod config;

pub use config::Config;

use clap::{App, Arg};
use std::io;
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
