// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod cli;
mod logger;
use self::cli::cli;
use async_std::task;
use forest_libp2p::{get_keypair, Libp2pService};
use libp2p::identity::{ed25519, Keypair};
use log::{info, trace};
use utils::write_to_file;

#[async_std::main]
async fn main() {
    logger::setup_logger();
    info!("Starting Forest");

    // Capture CLI inputs
    let config = cli().expect("CLI error");

    let net_keypair = match get_keypair(&"/.forest/libp2p/keypair") {
        Some(kp) => kp,
        None => {
            // Keypair not found, generate and save generated keypair
            let gen_keypair = ed25519::Keypair::generate();
            // Save Ed25519 keypair to file
            // TODO rename old file to keypair.old(?)
            if let Err(e) = write_to_file(&gen_keypair.encode(), &"/.forest/libp2p/", "keypair") {
                info!("Could not write keystore to disk!");
                trace!("Error {:?}", e);
            };
            Keypair::Ed25519(gen_keypair)
        }
    };

    let lp2p_service = Libp2pService::new(&config.network, net_keypair);

    task::block_on(async move {
        lp2p_service.run().await;
    });

    info!("Forest finish shutdown");
}
