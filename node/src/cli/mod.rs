use clap::{App, Arg};

pub(super) fn cli() {
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
        println!("Ferret was run!")
    }

    if let Some(ref config_file) = app.value_of("config") {
        println!("File path: {}", config_file);
    }
}
