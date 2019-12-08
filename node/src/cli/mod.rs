mod config;

pub use config::Config;

use clap::{App, Arg};
use node::utils::{read_file, read_toml};
use slog::Logger;
use std::io;

pub(super) fn cli(_log: &Logger) -> Result<Config, io::Error> {
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

    if let Some(config_file) = app.value_of("config") {
        // Read from config file
        let toml = read_file(config_file.to_string())?;

        // Parse and return the configuration file
        return Ok(read_toml(&toml)?);
    };
    // TODO in future parse all flags and append to a configuraiton object
    // Retrun defaults
    Ok(Config::default())
}
