// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod cli;
mod log;

use self::cli::cli;
use async_std::task;
use forest_libp2p::service::Libp2pService;
use slog::info;

#[async_std::main]
async fn main() {
    let log = log::setup_logging();
    info!(log, "Starting Forest");

    // Capture CLI inputs
    let config = cli(&log).expect("CLI error");

    let logger = log.clone();

    let lp2p_service = Libp2pService::new(logger, &config.network);

    task::block_on(async move {
        lp2p_service.run().await;
    });

    info!(log, "Forest finish shutdown");
}
