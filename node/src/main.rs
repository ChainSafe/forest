// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

mod cli;
mod log;

use self::cli::cli;
use async_std::task;
use ferret_libp2p::service::{Libp2pService, NetworkEvent};
use futures::channel::mpsc;
use futures::prelude::*;
use futures::stream::Stream;
use futures::stream::StreamExt;
use slog::info;
use std::error::Error;

#[async_std::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let log = log::setup_logging();
    info!(log, "Starting Ferret");

    // Capture CLI inputs
    let config = cli(&log).expect("CLI error");

    let logger = log.clone();
    let mut lp2p_service = Libp2pService::new(&logger, &config.network);

    task::block_on(async move {
        lp2p_service.run().await;
    });
    info!(log, "Ferret finish shutdown");
    Ok(())
}
