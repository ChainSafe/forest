use blocks;
use chain_sync;
use message_pool;
use storage_consensus;
fn main() {
    blocks::run();
    chain_sync::run();
    message_pool::run();
    storage_consensus::run();
}
