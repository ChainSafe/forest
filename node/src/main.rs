// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod cli;
mod log;

use self::cli::cli;
use forest_libp2p::service::NetworkEvent;
use futures::prelude::*;
use network::service::NetworkService;
use slog::info;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

fn main() {
    let log = log::setup_logging();
    info!(log, "Starting Forest");

    // Capture CLI inputs
    let config = cli(&log).expect("CLI error");

    // Create the tokio runtime
    let rt = Runtime::new().unwrap();

    // Create the channel so we can receive messages from NetworkService
    let (tx, _rx) = mpsc::unbounded_channel::<NetworkEvent>();
    // Create the default libp2p config
    // Start the NetworkService. Returns net_tx so  you can pass messages in.
    let (_network_service, _net_tx, _exit_tx) =
        NetworkService::new(&config.network, &log, tx, &rt.executor());

    rt.shutdown_on_idle().wait().unwrap();
    info!(log, "Forest finish shutdown");
}
