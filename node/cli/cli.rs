use clap::{App, Arg, SubCommand};

pub fn cli() {
    let app = App::new("Ferret")
        .version(crate_version!())
        .author("ChainSafe Systems <info@chainsafe.io>")
        .about("Filecoin implementation in Rust")
        /*
         * Flags
         */
        .arg(
            Arg::with_name("config")
                .long("config")
                .short("c")
                .help("A toml file containing relevant configurations.")
        )
        .get_matches();

    if app.is_present("Ferret") {
        println!("Ferret was run!")
    }

    if let Some(ref config_file) = matches.value_of("config") {
        println!("File path: {}", config_file);
    }
}
