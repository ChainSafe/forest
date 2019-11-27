mod cli;
mod utils;

use cli::cli;
use utils::{write_libp2p_id, get_libp2p_id};

fn main() {
    cli();
    write_libp2p_id("5");
    // let c = get_libp2p_id();
    // println!("{:?}", c);
}