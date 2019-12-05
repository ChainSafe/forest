mod config;

pub use config::Config;

use clap::{App, Arg};
use node::utils::{read_file, read_toml};
use slog::*;

pub(super) fn cli(log: &Logger) -> Config {
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

        // Parse and return the configuration file
        return match read_toml(&toml) {
            Ok(contents) => contents,
            Err(e) => panic!("{:?}", e),
        };
    };
    // TODO in future parse all flags and append to a configuraiton object
    // Retrun defaults
    Config::default()
}
