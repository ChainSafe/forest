// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod cli;
mod log;

use self::cli::cli;
use async_std::task;
use forest_libp2p::{get_keypair, Libp2pService};
use libp2p::identity::{ed25519, Keypair};
use slog::{info, trace};
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use utils::write_to_file;

#[async_std::main]
async fn main() {
    let log = log::setup_logging();
    info!(log, "Starting Forest");

    // Capture CLI inputs
    let config = cli(&log).expect("CLI error");

    let logger = log.clone();

    let net_keypair = match get_keypair(&log, &"/.forest/libp2p/keypair") {
        Some(kp) => kp,
        None => {
            // Keypair not found, generate and save generated keypair
            let gen_keypair = ed25519::Keypair::generate();
            // Save Ed25519 keypair to file
            // TODO rename old file to keypair.old(?)
            if let Err(e) = write_to_file(&gen_keypair.encode(), &"/.forest/libp2p/", "keypair") {
                info!(log, "Could not write keystore to disk!");
                trace!(log, "Error {:?}", e);
            };
            Keypair::Ed25519(gen_keypair)
        }
    };

    let running = Arc::new(AtomicUsize::new(0));
    let r = running.clone();
    ctrlc::set_handler(move || {
        let prev = r.fetch_add(1, Ordering::SeqCst);
        if prev == 0 {
            println!("Got interrupt, shutting down...");
        } else {
            process::exit(0);
        }
    })
    .expect("Error setting Ctrl-C handler");

    // Start libp2p service
    let p2p_service = Libp2pService::new(logger, &config.network, net_keypair);
    let p2p_thread = task::spawn(async {
        p2p_service.run().await;
    });

    loop {
        if running.load(Ordering::SeqCst) > 0 {
            // TODO change dropping threads to gracefully shutting down services
            // or implement drop on components with sensitive shutdown
            drop(p2p_thread);
            break;
        }
    }

    info!(log, "Forest finish shutdown");
}
