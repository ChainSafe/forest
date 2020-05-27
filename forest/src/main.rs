// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod cli;
mod logger;

use self::cli::{block_until_sigint, initialize_genesis};
use async_std::task;
use beacon::DrandBeacon;
use chain::ChainStore;
use chain_sync::ChainSyncer;
use db::RocksDb;
use forest_libp2p::{get_keypair, Libp2pService};
use libp2p::identity::{ed25519, Keypair};
use log::{info, trace};
use std::sync::Arc;
use structopt::StructOpt;
use utils::write_to_file;

fn main() {
    logger::setup_logger();
    info!("Starting Forest");

    // Capture CLI inputs
    let cli = cli::CLI::from_args();
    let mut config = cli.get_config().expect("CLI error");

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

    // Initialize database
    let mut db = RocksDb::new(config.data_dir + "/db");
    db.open().unwrap();
    let db = Arc::new(db);
    let mut chain_store = ChainStore::new(Arc::clone(&db));

    // Read Genesis file
    let (genesis, network_name) =
        initialize_genesis(&config.genesis_file, &mut chain_store).unwrap();

    // Libp2p service setup
    config.network.set_network_name(&network_name);
    let p2p_service = Libp2pService::new(&config.network, net_keypair);
    let network_rx = p2p_service.network_receiver();
    let network_send = p2p_service.network_sender();

    // Get Drand Coefficients
    let coeff = config.drand_dist_public.clone();

    // Start services
    let p2p_thread = task::spawn(async {
        p2p_service.run().await;
    });
    let sync_thread = task::spawn(async {
        // TODO: Interval is supposed to be consistent with fils epoch interval length, but not yet defined
        let beacon = DrandBeacon::new(coeff, genesis.blocks()[0].timestamp(), 1)
            .await
            .unwrap();

        // Initialize ChainSyncer
        let chain_syncer = ChainSyncer::new(
            chain_store,
            Arc::new(beacon),
            network_send,
            network_rx,
            genesis,
        )
        .unwrap();
        chain_syncer.start().await.unwrap();
    });

    // Block until ctrl-c is hit
    block_until_sigint();

    // Drop threads
    drop(p2p_thread);
    drop(sync_thread);

    info!("Forest finish shutdown");
}
