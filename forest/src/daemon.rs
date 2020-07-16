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
use log::{debug, info, trace};
use rpc::{start_rpc, RpcState};
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
    let p2p_service =
        Libp2pService::new(config.network, Arc::clone(&db), net_keypair, &network_name);
    let network_rx = p2p_service.network_receiver();
    let network_send = p2p_service.network_sender();

    // Get Drand Coefficients
    let coeff = config.drand_dist_public;

    // TODO: Interval is supposed to be consistent with fils epoch interval length, but not yet defined
    let beacon = DrandBeacon::new(coeff, genesis.blocks()[0].timestamp(), 1)
        .await
        .unwrap();

    // Initialize ChainSyncer
    let chain_syncer = ChainSyncer::new(
        chain_store,
        Arc::new(beacon),
        network_send.clone(),
        network_rx,
        genesis,
    )
    .unwrap();
    let bad_blocks = chain_syncer.bad_blocks_cloned();
    let sync_state = chain_syncer.sync_state_cloned();
    let sync_task = task::spawn(async {
        chain_syncer.start().await.unwrap();
    });

    // Start services
    let p2p_task = task::spawn(async {
        p2p_service.run().await;
    });

    let rpc_task = if config.enable_rpc {
        let db_rpc = Arc::clone(&db);
        let rpc_listen = format!("127.0.0.1:{}", &config.rpc_port);
        Some(task::spawn(async move {
            info!("JSON RPC Endpoint at {}", &rpc_listen);
            start_rpc(
                RpcState {
                    store: db_rpc,
                    bad_blocks,
                    sync_state,
                    network_send,
                    network_name,
                },
                &rpc_listen,
            )
            .await;
        }))
    } else {
        debug!("RPC disabled");
        None
    };

    // Block until ctrl-c is hit
    block_until_sigint().await;

    // Cancel all async services
    p2p_task.cancel().await;
    sync_task.cancel().await;
    if let Some(task) = rpc_task {
        task.cancel().await;
    }

    info!("Forest finish shutdown");
}
