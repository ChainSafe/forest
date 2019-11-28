mod cli;

use cli::cli;

use tokio::sync::mpsc;

use ferret_libp2p::config::Libp2pConfig;
use ferret_libp2p::service::NetworkEvent;
use network::service::*;

use futures::prelude::*;

use tokio;

use tokio::runtime::Runtime;

fn main() {
    cli();

    // TODO Everything below should be run in a function somewhere, but since we only have this
    // main right now, should be ok to leave here
    // Create the tokio runtime
    let rt = Runtime::new().unwrap();

    // Create the channel so we can receive messages from NetworkService
    let (tx, _rx) = mpsc::unbounded_channel::<NetworkEvent>();
    // Create the default libp2p config
    let netcfg = Libp2pConfig::default();
    // Start the NetworkService. Returns net_tx so  you can pass messages in.
    let (_network_service, _net_tx, _exit_tx) = NetworkService::new(&netcfg, tx, &rt.executor());

    rt.shutdown_on_idle().wait().unwrap();
}
