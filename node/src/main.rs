// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod cli;
mod log;

use self::cli::cli;
use async_std::task;
use forest_libp2p::service::Libp2pService;
use slog::info;
use libp2p::Swarm;

#[async_std::main]
async fn main() {
    let log = log::setup_logging();
    info!(log, "Starting Forest");

    // Capture CLI inputs
    let mut config = cli(&log).expect("CLI error");

    let logger = log.clone();
    config.network.listening_multiaddr = "/ip4/0.0.0.0/tcp/10006".to_owned();

    let lp2p_service = Libp2pService::new(logger, &config.network);


    task::block_on(async move {
        {
            for addr in Swarm::listeners(&lp2p_service.swarm) {
                println!("Listening on {:?}", addr);
            }
        }

        lp2p_service.run().await;
    });

    info!(log, "Forest finish shutdown");
}
