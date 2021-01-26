// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::cli::{block_until_sigint, Config};
use async_std::{channel::bounded, sync::RwLock, task};
use auth::{generate_priv_key, JWT_IDENTIFIER};
use chain::ChainStore;
use chain_sync::ChainSyncer;
use fil_types::verifier::FullVerifier;
use flo_stream::{MessagePublisher, Publisher};
use forest_libp2p::{get_keypair, Libp2pService};
use genesis::{import_chain, initialize_genesis};
use libp2p::identity::{ed25519, Keypair};
use log::{debug, info, trace};
use message_pool::{MessagePool, MpoolConfig, MpoolRpcProvider};
use paramfetch::{get_params_default, SectorSizeOpt};
use rpc::{start_rpc, RpcState};
use state_manager::StateManager;
use std::sync::Arc;
use utils::write_to_file;
use wallet::{KeyStore, PersistentKeyStore};

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
    let mut ks = PersistentKeyStore::new(config.data_dir.to_string()).unwrap();
    if ks.get(JWT_IDENTIFIER).is_err() {
        ks.put(JWT_IDENTIFIER.to_owned(), generate_priv_key())
            .unwrap();
    }
    let keystore = Arc::new(RwLock::new(ks));

    // Initialize database (RocksDb will be default if both features enabled)
    #[cfg(all(feature = "sled", not(feature = "rocksdb")))]
    let db = db::sled::SledDb::open(config.data_dir + "/sled").unwrap();

    #[cfg(feature = "rocksdb")]
    let db = db::rocks::RocksDb::open(config.data_dir + "/db").unwrap();

    let db = Arc::new(db);

    // Initialize StateManager
    let chain_store = Arc::new(ChainStore::new(Arc::clone(&db)));
    let state_manager = Arc::new(StateManager::new(Arc::clone(&chain_store)));

    let publisher = chain_store.publisher();

    // Read Genesis file
    // * When snapshot command implemented, this genesis does not need to be initialized
    let (genesis, network_name) = initialize_genesis(config.genesis_file.as_ref(), &state_manager)
        .await
        .unwrap();

    let validate_height = if config.snapshot { None } else { Some(0) };
    // Sync from snapshot
    if let Some(path) = &config.snapshot_path {
        import_chain::<FullVerifier, _>(&state_manager, path, validate_height, config.skip_load)
            .await
            .unwrap();
    }

    // Fetch and ensure verification keys are downloaded
    get_params_default(SectorSizeOpt::Keys, false)
        .await
        .unwrap();

    // Libp2p service setup
    let p2p_service = Libp2pService::new(
        config.network,
        Arc::clone(&chain_store),
        net_keypair,
        &network_name,
    );
    let network_rx = p2p_service.network_receiver();
    let network_send = p2p_service.network_sender();

    // Initialize mpool
    let subscriber = publisher.write().await.subscribe();
    let provider = MpoolRpcProvider::new(subscriber, Arc::clone(&state_manager));
    let mpool = Arc::new(
        MessagePool::new(
            provider,
            network_name.clone(),
            network_send.clone(),
            MpoolConfig::load_config(db.as_ref()).unwrap(),
        )
        .await
        .unwrap(),
    );

    let beacon = Arc::new(
        networks::beacon_schedule_default(genesis.min_timestamp())
            .await
            .unwrap(),
    );

    // Initialize ChainSyncer
    let chain_syncer = ChainSyncer::<_, _, FullVerifier, _>::new(
        Arc::clone(&state_manager),
        beacon.clone(),
        Arc::clone(&mpool),
        network_send.clone(),
        network_rx,
        Arc::new(genesis),
        config.sync,
    )
    .unwrap();
    let bad_blocks = chain_syncer.bad_blocks_cloned();
    let sync_state = chain_syncer.sync_state_cloned();
    let (worker_tx, worker_rx) = bounded(20);
    let worker_tx_clone = worker_tx.clone();
    let sync_task = task::spawn(async move {
        chain_syncer.start(worker_tx_clone, worker_rx).await;
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
            start_rpc::<_, _, _, FullVerifier>(
                RpcState {
                    state_manager,
                    keystore: keystore_rpc,
                    mpool,
                    bad_blocks,
                    sync_state,
                    network_send,
                    network_name,
                    events_pubsub: Arc::new(RwLock::new(Publisher::new(1000))),
                    beacon,
                    chain_store,
                    new_mined_block_tx: worker_tx,
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

#[cfg(test)]
#[cfg(not(any(feature = "interopnet", feature = "devnet")))]
mod test {
    use super::*;
    use db::MemoryDB;

    #[async_std::test]
    async fn import_snapshot_from_file() {
        let db = Arc::new(MemoryDB::default());
        let cs = Arc::new(ChainStore::new(db));
        let sm = Arc::new(StateManager::new(cs));
        import_chain::<FullVerifier, _>(&sm, "test_files/chain4.car", None, false)
            .await
            .expect("Failed to import chain");
    }
    #[async_std::test]
    async fn import_chain_from_file() {
        let db = Arc::new(MemoryDB::default());
        let cs = Arc::new(ChainStore::new(db));
        let sm = Arc::new(StateManager::new(cs));
        import_chain::<FullVerifier, _>(&sm, "test_files/chain4.car", Some(0), false)
            .await
            .expect("Failed to import chain");
    }
}
