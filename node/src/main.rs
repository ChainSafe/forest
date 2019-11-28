mod cli;
mod log;
use slog::*;

use cli::cli;
#[derive(Debug)]
struct A {
    a:i32,
    b:String,
}
fn main() {
    cli();
    let log = log::setup_logging();
    info!(log, "Starting Ferret");

}
