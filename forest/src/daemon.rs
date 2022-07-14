// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::cli::{block_until_sigint, Config};
use auth::{create_token, generate_priv_key, ADMIN, JWT_IDENTIFIER};
use chain::ChainStore;
use chain_sync::ChainMuxer;
use fil_types::verifier::FullVerifier;
use forest_libp2p::{get_keypair, Libp2pConfig, Libp2pService};
use genesis::{get_network_name_from_genesis, import_chain, read_genesis_header};
use message_pool::{MessagePool, MpoolConfig, MpoolRpcProvider};
use paramfetch::{get_params_default, set_proofs_parameter_cache_dir_env, SectorSizeOpt};
use rpc::start_rpc;
use rpc_api::data_types::RPCState;
use state_manager::StateManager;
use utils::write_to_file;
use wallet::ENCRYPTED_KEYSTORE_NAME;
use wallet::{KeyStore, KeyStoreConfig};

use async_std::{channel::bounded, sync::RwLock, task};
use libp2p::identity::{ed25519, Keypair};
use log::{debug, error, info, trace, warn};
use rpassword::read_password;

use db::rocks::RocksDb;
use std::io::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::time;

/// Starts daemon process
pub(super) async fn start(config: Config) {
    info!(
        "Starting Forest daemon, version {}",
        option_env!("FOREST_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
    );

    let path: PathBuf = config.data_dir.join("libp2p");
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

            let data_dir = PathBuf::from(&config.data_dir).join(ENCRYPTED_KEYSTORE_NAME);
            if !data_dir.exists() {
                print!("Confirm passphrase: ");
                std::io::stdout().flush().unwrap();

                if passphrase != read_password().unwrap() {
                    error!("Passphrases do not match. Please retry.");
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
                    error!("Incorrect passphrase entered. Please try again.")
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

    // Start Prometheus server port
    let prometheus_server_task = task::spawn(metrics::init_prometheus(
        (format!("127.0.0.1:{}", config.metrics_port))
            .parse()
            .unwrap(),
        db_path(&config)
            .into_os_string()
            .into_string()
            .expect("Failed converting the path to db"),
    ));

    // Print admin token
    let ki = ks.get(JWT_IDENTIFIER).unwrap();
    let token = create_token(ADMIN.to_owned(), ki.private_key()).unwrap();
    info!("Admin token: {}", token);

    let keystore = Arc::new(RwLock::new(ks));

    #[cfg(feature = "rocksdb")]
    let db = db::rocks::RocksDb::open(db_path(&config), &config.rocks_db)
        .expect("Opening RocksDB must succeed");

    let db = Arc::new(db);

    // Initialize ChainStore
    let chain_store = Arc::new(ChainStore::new(Arc::clone(&db)));

    let publisher = chain_store.publisher();

    // Read Genesis file
    // * When snapshot command implemented, this genesis does not need to be initialized
    let genesis = read_genesis_header(
        config.genesis_file.as_ref(),
        config.chain.genesis_bytes(),
        &chain_store,
    )
    .await
    .unwrap();
    chain_store.set_genesis(&genesis.blocks()[0]).unwrap();

    // Initialize StateManager
    let sm = StateManager::new(Arc::clone(&chain_store), Arc::new(config.chain.clone()))
        .await
        .unwrap();
    let state_manager = Arc::new(sm);

    let network_name = get_network_name_from_genesis(&genesis, &state_manager)
        .await
        .unwrap();

    info!("Using network :: {}", network_name);

    sync_from_snapshot(&config, &state_manager).await;

    set_proofs_parameter_cache_dir_env(&config.data_dir);

    // Fetch and ensure verification keys are downloaded
    get_params_default(&config.data_dir, SectorSizeOpt::Keys, false)
        .await
        .unwrap();

    // Override bootstrap peers
    let config = if config.network.bootstrap_peers.is_empty() {
        let bootstrap_peers = config
            .chain
            .bootstrap_peers
            .iter()
            .map(|node| node.parse().unwrap())
            .collect();
        Config {
            network: Libp2pConfig {
                bootstrap_peers,
                ..config.network
            },
            ..config
        }
    } else {
        config
    };

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
            (*state_manager.chain_config).clone(),
        )
        .await
        .unwrap(),
    );

    // Initialize ChainMuxer
    let (tipset_sink, tipset_stream) = bounded(20);
    let chain_muxer_tipset_sink = tipset_sink.clone();
    let chain_muxer = ChainMuxer::<_, _, FullVerifier, _>::new(
        Arc::clone(&state_manager),
        state_manager.beacon_schedule(),
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
            info!("JSON-RPC endpoint started at {}", &rpc_listen);
            start_rpc::<_, _, FullVerifier>(
                Arc::new(RPCState {
                    state_manager: Arc::clone(&state_manager),
                    keystore: keystore_rpc,
                    mpool,
                    bad_blocks,
                    sync_state,
                    network_send,
                    network_name,
                    beacon: state_manager.beacon_schedule(), // TODO: the RPCState can fetch this itself from the StateManager
                    chain_store,
                    new_mined_block_tx: tipset_sink,
                }),
                &rpc_listen,
            )
            .await
        }))
    } else {
        debug!("RPC disabled.");
        None
    };

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

    info!("Forest finish shutdown.");
}

async fn sync_from_snapshot(config: &Config, state_manager: &Arc<StateManager<RocksDb>>) {
    if let Some(path) = &config.snapshot_path {
        let stopwatch = time::Instant::now();
        let validate_height = if config.snapshot {
            config.snapshot_height
        } else {
            Some(0)
        };
        import_chain::<FullVerifier, _>(state_manager, path, validate_height, config.skip_load)
            .await
            .expect("Failed miserably while importing chain from snapshot");
        debug!("Imported snapshot in: {}s", stopwatch.elapsed().as_secs());
    }
}

fn db_path(config: &Config) -> PathBuf {
    chain_path(config).join("db")
}

fn chain_path(config: &Config) -> PathBuf {
    PathBuf::from(&config.data_dir).join(&config.chain.name)
}

#[cfg(test)]
#[cfg(not(any(feature = "interopnet", feature = "devnet")))]
mod test {
    use super::*;
    use address::Address;
    use blocks::BlockHeader;
    use db::MemoryDB;
    use networks::ChainConfig;

    #[async_std::test]
    async fn import_snapshot_from_file() {
        let db = Arc::new(MemoryDB::default());
        let cs = Arc::new(ChainStore::new(db));
        let genesis_header = BlockHeader::builder()
            .miner_address(Address::new_id(0))
            .timestamp(7777)
            .build()
            .unwrap();
        cs.set_genesis(&genesis_header).unwrap();
        let chain_config = Arc::new(ChainConfig::default());
        let sm = Arc::new(StateManager::new(cs, chain_config).await.unwrap());
        import_chain::<FullVerifier, _>(&sm, "test_files/chain4.car", None, false)
            .await
            .expect("Failed to import chain");
    }

    // FIXME: This car file refers to actors that are not available in FVM yet.
    //        See issue: https://github.com/ChainSafe/forest/issues/1452
    // #[async_std::test]
    // async fn import_chain_from_file() {
    //     let db = Arc::new(MemoryDB::default());
    //     let cs = Arc::new(ChainStore::new(db));
    //     let genesis_header = BlockHeader::builder()
    //         .miner_address(Address::new_id(0))
    //         .timestamp(7777)
    //         .build()
    //         .unwrap();
    //     cs.set_genesis(&genesis_header).unwrap();
    //     let sm = Arc::new(StateManager::new(cs).await.unwrap());
    //     import_chain::<FullVerifier, _>(&sm, "test_files/chain4.car", Some(0), false)
    //         .await
    //         .expect("Failed to import chain");
    // }
}
