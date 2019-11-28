mod cli;
mod log;
use slog::*;

use cli::cli;

fn main() {
    cli();
    let log = log::setup_logging();
    info!(log, "Starting Ferret");
}
