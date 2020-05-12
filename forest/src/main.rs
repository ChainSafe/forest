// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod cli;
mod logger;

use self::cli::{block_until_sigint, initialize_genesis};
use async_std::task;
use beacon::{DistPublic, DrandBeacon};
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

    // Start services
    let p2p_thread = task::spawn(async {
        p2p_service.run().await;
    });
    let sync_thread = task::spawn(async {
        let coeffs = [
            hex::decode("82c279cce744450e68de98ee08f9698a01dd38f8e3be3c53f2b840fb9d09ad62a0b6b87981e179e1b14bc9a2d284c985").unwrap(),
            hex::decode("82d51308ad346c686f81b8094551597d7b963295cbf313401a93df9baf52d5ae98a87745bee70839a4d6e65c342bd15b").unwrap(),
            hex::decode("94eebfd53f4ba6a3b8304236400a12e73885e5a781509a5c8d41d2e8b476923d8ea6052649b3c17282f596217f96c5de").unwrap(),
            hex::decode("8dc4231e42b4edf39e86ef1579401692480647918275da767d3e558c520d6375ad953530610fd27daf110187877a65d0").unwrap(),
        ];
        let dist_pub = DistPublic {
            coefficients: coeffs,
        };
        let beacon = DrandBeacon::new(dist_pub, genesis.blocks()[0].timestamp(), 1)
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
