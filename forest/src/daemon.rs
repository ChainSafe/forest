// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::cli::{block_until_sigint, initialize_genesis, Config};
use async_std::task;
use beacon::DrandBeacon;
use chain::ChainStore;
use chain_sync::ChainSyncer;
use db::RocksDb;
use forest_libp2p::{get_keypair, Libp2pService};
use libp2p::identity::{ed25519, Keypair};
use log::{info, trace};
use rpc::start_rpc;
use std::sync::Arc;
use utils::write_to_file;

/// Starts daemon process
pub(super) async fn start(config: Config) {
    info!("Starting Forest daemon");
    let net_keypair = get_keypair(&format!("{}{}", &config.data_dir, "/libp2p/keypair"))
        .unwrap_or_else(|| {
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
        });

    // Initialize database
    let mut db = RocksDb::new(config.data_dir + "/db");
    db.open().unwrap();
    let db = Arc::new(db);
    let mut chain_store = ChainStore::new(Arc::clone(&db));

    // Read Genesis file
    let (genesis, network_name) =
        initialize_genesis(&config.genesis_file, &mut chain_store).unwrap();

    // Libp2p service setup
    let p2p_service = Libp2pService::new(config.network, net_keypair, &network_name);
    let network_rx = p2p_service.network_receiver();
    let network_send = p2p_service.network_sender();

    // Get Drand Coefficients
    let coeff = config.drand_dist_public;

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

    let db_rpc = Arc::clone(&db);
    let rpc_thread = task::spawn(async {
        start_rpc(db_rpc).await;
    });

    // Block until ctrl-c is hit
    block_until_sigint().await;

    // Cancel all async services
    rpc_thread.cancel().await;
    p2p_thread.cancel().await;
    sync_thread.cancel().await;

    info!("Forest finish shutdown");
}
