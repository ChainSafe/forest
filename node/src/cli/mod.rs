use clap::{App, Arg};
use slog::*;

use super::utils::{read_file, read_toml};

struct Config {
    port: u8,
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
        let toml = match read_file(config_file.to_owned()) {
            Ok(contents) => contents,
            Err(e) => panic!("{:?}", e),
        };
        let config: Config = match read_toml(toml) {
            Ok(contents) => contents,
            Err(e) => panic!("{:?}", e),
        }
    }
}
