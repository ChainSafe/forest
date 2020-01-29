// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

mod cli;
mod log;

use self::cli::cli;
use ferret_libp2p::service::NetworkEvent;
use futures::prelude::*;
use futures::stream::StreamExt;
use futures::stream::Stream;
use futures::channel::mpsc;
use network::service::NetworkService;
use slog::info;
use std::error::Error;
use async_std::task;

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let log = log::setup_logging();
    info!(log, "Starting Ferret");

    // Capture CLI inputs
    let config = cli(&log).expect("CLI error");

    // Create the tokio runtime
    // Create the channel so we can receive messages from NetworkService
    let (tx, mut rx) = mpsc::unbounded::<NetworkEvent>();
    // Create the default libp2p config
    // Start the NetworkService. Returns net_tx so  you can pass messages in.
    let (_network_service, _net_tx, _exit_tx) =
        NetworkService::new(&config.network, &log, tx);

    task::block_on(
        async move {
            while let Some(ev) = rx.next().await {
                println!("{:?}", ev);
            }
        }
    );
    info!(log, "Ferret finish shutdown");
    Ok(())
}
