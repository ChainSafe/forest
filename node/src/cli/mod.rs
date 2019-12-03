use clap::{App, Arg};
use slog::*;

use crate::utils::{read_file, read_toml};
use serde_derive::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(default)]
struct Config {
    network: Network,
}

#[derive(Debug, Deserialize)]
struct Network {
    port: u16,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            network: Network { port: 8545 },
        }
    }
}

pub(super) fn cli(log: &Logger) {
    let app = App::new("Ferret")
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
        .get_matches();

    if app.is_present("Ferret") {
        info!(log, "Ferret was run!");
    }

    if let Some(ref config_file) = app.value_of("config") {
        // Read from config file
        let toml = match read_file(config_file.to_string()) {
            Ok(contents) => contents,
            Err(e) => panic!("{:?}", e),
        };

        // Parse config file
        let _: Config = match read_toml(&toml) {
            Ok(contents) => contents,
            Err(e) => panic!("{:?}", e),
        };
    };
}
