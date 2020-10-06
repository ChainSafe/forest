// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::cli::{block_until_sigint, initialize_genesis, Config};
use actor::EPOCH_DURATION_SECONDS;
use async_std::sync::RwLock;
use async_std::task;
use beacon::{DrandBeacon, DEFAULT_DRAND_URL};
use chain::ChainStore;
use chain_sync::ChainSyncer;
use db::RocksDb;
use fil_types::verifier::FullVerifier;
use flo_stream::{MessagePublisher, Publisher};
use forest_libp2p::{get_keypair, Libp2pService};
use libp2p::identity::{ed25519, Keypair};
use log::{debug, info, trace};
use message_pool::{MessagePool, MpoolConfig, MpoolRpcProvider};
use rpc::{start_rpc, RpcState};
use state_manager::StateManager;
use std::sync::Arc;
use utils::write_to_file;
use wallet::PersistentKeyStore;

/// Number of tasks spawned for sync workers.
// TODO benchmark and/or add this as a config option.
const WORKER_TASKS: usize = 3;

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

    // Initialize keystore
    let keystore = Arc::new(RwLock::new(
        PersistentKeyStore::new(config.data_dir.to_string()).unwrap(),
    ));

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

    // Initialize StateManager
    let state_manager = Arc::new(StateManager::new(Arc::clone(&db)));

    // Initialize mpool
    let publisher = chain_store.publisher();
    let subscriber = publisher.write().await.subscribe();
    let provider = MpoolRpcProvider::new(subscriber, Arc::clone(&state_manager));
    let mpool = Arc::new(
        MessagePool::new(
            provider,
            network_name.clone(),
            MpoolConfig::load_config(db.as_ref()).unwrap(),
        )
        .await
        .unwrap(),
    );

    // Get Drand Coefficients
    let coeff = config.drand_public;

    let beacon = DrandBeacon::new(
        DEFAULT_DRAND_URL,
        coeff,
        genesis.blocks()[0].timestamp(),
        EPOCH_DURATION_SECONDS as u64,
    )
    .await
    .unwrap();

    // Initialize ChainSyncer
    let chain_store_arc = Arc::new(chain_store);
    // TODO allow for configuring validation strategy (defaulting to full validation)
    let chain_syncer = ChainSyncer::<_, _, FullVerifier>::new(
        chain_store_arc.clone(),
        Arc::clone(&state_manager),
        Arc::new(beacon),
        network_send.clone(),
        network_rx,
        Arc::new(genesis),
    )
    .unwrap();
    let bad_blocks = chain_syncer.bad_blocks_cloned();
    let sync_state = chain_syncer.sync_state_cloned();
    let sync_task = task::spawn(async {
        chain_syncer.start(WORKER_TASKS).await;
    });

    // Start services
    let p2p_task = task::spawn(async {
        p2p_service.run().await;
    });
    let rpc_task = if config.enable_rpc {
        let keystore_rpc = Arc::clone(&keystore);
        let rpc_listen = format!("127.0.0.1:{}", &config.rpc_port);
        Some(task::spawn(async move {
            info!("JSON RPC Endpoint at {}", &rpc_listen);
            start_rpc(
                RpcState {
                    state_manager,
                    keystore: keystore_rpc,
                    mpool,
                    bad_blocks,
                    sync_state,
                    network_send,
                    network_name,
                    chain_store: chain_store_arc,
                    events_pubsub: Arc::new(RwLock::new(Publisher::new(1000))),
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

    let keystore_write = task::spawn(async move {
        keystore.read().await.flush().unwrap();
    });

    // Cancel all async services
    p2p_task.cancel().await;
    sync_task.cancel().await;
    if let Some(task) = rpc_task {
        task.cancel().await;
    }
    keystore_write.await;

    info!("Forest finish shutdown");
}
