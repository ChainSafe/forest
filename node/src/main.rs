mod cli;
mod log;
use slog::*;

use cli::cli;

fn main() {
    let log = log::setup_logging();
    info!(log, "Starting Ferret");
    cli(&log);
}
