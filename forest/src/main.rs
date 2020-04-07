// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use self::cli::cli;
mod cli;
mod logger;
use async_std::task;
use chain_sync::ChainSyncer;
use db::RocksDb;
use forest_libp2p::{get_keypair, Libp2pService};
use libp2p::identity::{ed25519, Keypair};
use log::{info, trace};
use std::cell::RefCell;
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use utils::write_to_file;

// Blocks current thread until ctrl-c is received
fn block_until_sigint() {
    let (ctrlc_send, ctrlc_oneshot) = futures::channel::oneshot::channel();
    let ctrlc_send_c = RefCell::new(Some(ctrlc_send));

    let running = Arc::new(AtomicUsize::new(0));
    ctrlc::set_handler(move || {
        let prev = running.fetch_add(1, Ordering::SeqCst);
        if prev == 0 {
            println!("Got interrupt, shutting down...");
            // Send sig int in channel to blocking task
            if let Some(ctrlc_send) = ctrlc_send_c.try_borrow_mut().unwrap().take() {
                ctrlc_send.send(()).expect("Error sending ctrl-c message");
            }
        } else {
            process::exit(0);
        }
    })
    .expect("Error setting Ctrl-C handler");

    task::block_on(ctrlc_oneshot).unwrap();
}

fn main() {
    logger::setup_logger();
    info!("Starting Forest");

    // Capture CLI inputs
    let config = cli().expect("CLI error");

    let net_keypair = match get_keypair(&format!("{}{}", &config.data_dir, "/libp2p/keypair")) {
        Some(kp) => kp,
        None => {
            // Keypair not found, generate and save generated keypair
            let gen_keypair = ed25519::Keypair::generate();
            // Save Ed25519 keypair to file
            // TODO rename old file to keypair.old(?)
            if let Err(e) = write_to_file(
                &gen_keypair.encode(),
                &format!("{}{}", &config.data_dir, "/libp2p/"),
                "keypair",
            ) {
                info!("Could not write keystore to disk!");
                trace!("Error {:?}", e);
            };
            Keypair::Ed25519(gen_keypair)
        }
    };

    // Start libp2p service
    let p2p_service = Libp2pService::new(&config.network, net_keypair);
    let network_rx = p2p_service.network_receiver();
    let network_send = p2p_service.network_sender();

    // Start services
    let p2p_thread = task::spawn(async {
        p2p_service.run().await;
    });
    let sync_thread = task::spawn(async {
        // Initialize database
        let mut db = RocksDb::new(config.data_dir + "/db");
        db.open().unwrap();

        let chain_syncer = ChainSyncer::new(Arc::new(db), network_send, network_rx).unwrap();
        chain_syncer.start().await.unwrap();
    });

    // Block until ctrl-c is hit
    block_until_sigint();

    // Drop threads
    drop(p2p_thread);
    drop(sync_thread);

    info!("Forest finish shutdown");
}
