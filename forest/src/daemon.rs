// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::cli::{block_until_sigint, Config};
use auth::{create_token, generate_priv_key, ADMIN, JWT_IDENTIFIER};
use chain::ChainStore;
use chain_sync::ChainMuxer;
use fil_types::verifier::FullVerifier;
use forest_libp2p::{get_keypair, Libp2pService};
use genesis::{import_chain, initialize_genesis};
use message_pool::{MessagePool, MpoolConfig, MpoolRpcProvider};
use paramfetch::{get_params_default, SectorSizeOpt};
use rpc::start_rpc;
use rpc_api::data_types::RPCState;
use state_manager::StateManager;
use utils::write_to_file;
use wallet::ENCRYPTED_KEYSTORE_NAME;
use wallet::{KeyStore, KeyStoreConfig};

use async_std::{channel::bounded, sync::RwLock, task};
use libp2p::identity::{ed25519, Keypair};
use log::{debug, info, trace, warn};
use rpassword::read_password;

use std::io::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;

/// Starts daemon process
pub(super) async fn start(config: Config) {
    // Set the Address network prefix
    #[cfg(feature = "testnet")]
    address::NETWORK_DEFAULT
        .set(address::Network::Testnet)
        .unwrap();
    #[cfg(not(feature = "testnet"))]
    address::NETWORK_DEFAULT
        .set(address::Network::Mainnet)
        .unwrap();

    info!(
        "Starting Forest daemon, version {}",
        option_env!("FOREST_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
    );

    let path: PathBuf = [&config.data_dir, "libp2p"].iter().collect();
    let net_keypair = get_keypair(&path.join("keypair")).unwrap_or_else(|| {
        // Keypair not found, generate and save generated keypair
        let gen_keypair = ed25519::Keypair::generate();
        // Save Ed25519 keypair to file
        // TODO rename old file to keypair.old(?)
        match write_to_file(&gen_keypair.encode(), &path, "keypair") {
            Ok(file) => {
                // Restrict permissions on files containing private keys
                #[cfg(unix)]
                utils::set_user_perm(&file).expect("Set user perms on unix systems");
            }
            Err(e) => {
                info!("Could not write keystore to disk!");
                trace!("Error {:?}", e);
            }
        };
        Keypair::Ed25519(gen_keypair)
    });

    let mut ks = if config.encrypt_keystore {
        loop {
            print!("Enter the keystore passphrase: ");
            std::io::stdout().flush().unwrap();

            let passphrase = read_password().expect("Error reading passphrase");

            let mut data_dir = PathBuf::from(&config.data_dir);
            data_dir.push(ENCRYPTED_KEYSTORE_NAME);

            if !data_dir.exists() {
                print!("Confirm passphrase: ");
                std::io::stdout().flush().unwrap();

                if passphrase != read_password().unwrap() {
                    println!("Passphrases do not match. Please retry.");
                    continue;
                }
            }

            let key_store_init_result = KeyStore::new(KeyStoreConfig::Encrypted(
                PathBuf::from(&config.data_dir),
                passphrase,
            ));

            match key_store_init_result {
                Ok(ks) => break ks,
                Err(_) => {
                    log::error!("Incorrect passphrase entered. Please try again.")
                }
            };
        }
    } else {
        warn!("Warning: Keystore encryption disabled!");
        KeyStore::new(KeyStoreConfig::Persistent(PathBuf::from(&config.data_dir)))
            .expect("Error initializing keystore")
    };

    if ks.get(JWT_IDENTIFIER).is_err() {
        ks.put(JWT_IDENTIFIER.to_owned(), generate_priv_key())
            .unwrap();
    }

    // Print admin token
    let ki = ks.get(JWT_IDENTIFIER).unwrap();
    let token = create_token(ADMIN.to_owned(), ki.private_key()).unwrap();
    println!("Admin token: {}", token);

    let keystore = Arc::new(RwLock::new(ks));

    // Initialize database (RocksDb will be default if both features enabled)
    #[cfg(all(feature = "sled", not(feature = "rocksdb")))]
    let db = db::sled::SledDb::open(format!("{}/{}", config.data_dir, "/sled"))
        .expect("Opening SledDB must succeed");

    #[cfg(feature = "rocksdb")]
    let db = db::rocks::RocksDb::open(format!("{}/{}", config.data_dir.clone(), "db"))
        .expect("Opening RocksDB must succeed");

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
    let provider = MpoolRpcProvider::new(publisher.clone(), Arc::clone(&state_manager));
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

    // Initialize ChainMuxer
    let (tipset_sink, tipset_stream) = bounded(20);
    let chain_muxer_tipset_sink = tipset_sink.clone();
    let chain_muxer = ChainMuxer::<_, _, FullVerifier, _>::new(
        Arc::clone(&state_manager),
        beacon.clone(),
        Arc::clone(&mpool),
        network_send.clone(),
        network_rx,
        Arc::new(genesis),
        chain_muxer_tipset_sink,
        tipset_stream,
        config.sync,
    )
    .expect("Instantiating the ChainMuxer must succeed");
    let bad_blocks = chain_muxer.bad_blocks_cloned();
    let sync_state = chain_muxer.sync_state_cloned();
    let sync_task = task::spawn(chain_muxer);

    // Start services
    let p2p_task = task::spawn(async {
        p2p_service.run().await;
    });
    let rpc_task = if config.enable_rpc {
        let keystore_rpc = Arc::clone(&keystore);
        let rpc_listen = format!("127.0.0.1:{}", &config.rpc_port);
        Some(task::spawn(async move {
            info!("JSON RPC Endpoint at {}", &rpc_listen);
            start_rpc::<_, _, FullVerifier>(
                Arc::new(RPCState {
                    state_manager,
                    keystore: keystore_rpc,
                    mpool,
                    bad_blocks,
                    sync_state,
                    network_send,
                    network_name,
                    beacon,
                    chain_store,
                    new_mined_block_tx: tipset_sink,
                }),
                &rpc_listen,
            )
            .await
        }))
    } else {
        debug!("RPC disabled");
        None
    };

    // Start Prometheus server port
    let prometheus_server_task = task::spawn(metrics::init_prometheus(
        (format!("127.0.0.1:{}", config.metrics_port))
            .parse()
            .unwrap(),
        format!("{}/{}", config.data_dir.clone(), "db"),
    ));

    // Block until ctrl-c is hit
    block_until_sigint().await;

    let keystore_write = task::spawn(async move {
        keystore.read().await.flush().unwrap();
    });

    // Cancel all async services
    prometheus_server_task.cancel().await;
    sync_task.cancel().await;
    p2p_task.cancel().await;
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
