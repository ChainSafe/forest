// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::cli::{set_sigint_handler, Config, FOREST_VERSION_STRING};
use crate::cli_error_and_die;
use forest_auth::{create_token, generate_priv_key, ADMIN, JWT_IDENTIFIER};
use forest_chain::ChainStore;
use forest_chain_sync::consensus::SyncGossipSubmitter;
use forest_chain_sync::ChainMuxer;
use forest_db::rocks::RocksDb;
use forest_fil_types::verifier::FullVerifier;
use forest_genesis::{get_network_name_from_genesis, import_chain, read_genesis_header};
use forest_key_management::ENCRYPTED_KEYSTORE_NAME;
use forest_key_management::{KeyStore, KeyStoreConfig};
use forest_libp2p::PeerId;
use forest_libp2p::{ed25519, get_keypair, Keypair, Libp2pConfig, Libp2pService};
use forest_message_pool::{MessagePool, MpoolConfig, MpoolRpcProvider};
use forest_rpc::start_rpc;
use forest_rpc_api::data_types::RPCState;
use forest_state_manager::StateManager;
use forest_utils::write_to_file;
use fvm_shared::version::NetworkVersion;

use async_std::{channel::bounded, net::TcpListener, sync::RwLock, task, task::JoinHandle};
use futures::{select, FutureExt};
use log::{debug, error, info, trace, warn};
use rpassword::read_password;

use std::io::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::time;

// Initialize Consensus
#[cfg(not(any(feature = "forest_fil_cns", feature = "forest_deleg_cns")))]
compile_error!("No consensus feature enabled; use e.g. `--feature forest_fil_cns` to pick one.");

// Default consensus
#[cfg(all(feature = "forest_fil_cns", not(any(feature = "forest_deleg_cns"))))]
use forest_fil_cns::composition as cns;
// Custom consensus.
#[cfg(feature = "forest_deleg_cns")]
use forest_deleg_cns::composition as cns;

/// Starts daemon process
pub(super) async fn start(config: Config) {
    let mut ctrlc_oneshot = set_sigint_handler();

    info!(
        "Starting Forest daemon, version {}",
        FOREST_VERSION_STRING.as_str()
    );

    let path: PathBuf = config.client.data_dir.join("libp2p");
    let net_keypair = get_keypair(&path.join("keypair")).unwrap_or_else(|| {
        // Keypair not found, generate and save generated keypair
        let gen_keypair = ed25519::Keypair::generate();
        // Save Ed25519 keypair to file
        // TODO rename old file to keypair.old(?)
        match write_to_file(&gen_keypair.encode(), &path, "keypair") {
            Ok(file) => {
                // Restrict permissions on files containing private keys
                #[cfg(unix)]
                forest_utils::set_user_perm(&file).expect("Set user perms on unix systems");
            }
            Err(e) => {
                info!("Could not write keystore to disk!");
                trace!("Error {:?}", e);
            }
        };
        Keypair::Ed25519(gen_keypair)
    });

    // Hint at the multihash which has to go in the `/p2p/<multihash>` part of the peer's multiaddress.
    // Useful if others want to use this node to bootstrap from.
    info!("PeerId: {}", PeerId::from(net_keypair.public()));

    let mut ks = if config.client.encrypt_keystore {
        loop {
            print!("Enter the keystore passphrase: ");
            std::io::stdout().flush().unwrap();

            let passphrase = read_password().expect("Error reading passphrase");

            let data_dir = PathBuf::from(&config.client.data_dir).join(ENCRYPTED_KEYSTORE_NAME);
            if !data_dir.exists() {
                print!("Confirm passphrase: ");
                std::io::stdout().flush().unwrap();

                if passphrase != read_password().unwrap() {
                    error!("Passphrases do not match. Please retry.");
                    continue;
                }
            }

            let key_store_init_result = KeyStore::new(KeyStoreConfig::Encrypted(
                PathBuf::from(&config.client.data_dir),
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
        KeyStore::new(KeyStoreConfig::Persistent(PathBuf::from(
            &config.client.data_dir,
        )))
        .expect("Error initializing keystore")
    };

    if ks.get(JWT_IDENTIFIER).is_err() {
        ks.put(JWT_IDENTIFIER.to_owned(), generate_priv_key())
            .unwrap();
    }

    // Start Prometheus server port
    let prometheus_listener = TcpListener::bind(config.client.metrics_address)
        .await
        .unwrap_or_else(|_| {
            cli_error_and_die(
                format!("could not bind to {}", config.client.metrics_address),
                1,
            )
        });
    info!(
        "Prometheus server started at {}",
        config.client.metrics_address
    );
    let prometheus_server_task = task::spawn(forest_metrics::init_prometheus(
        prometheus_listener,
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

    let db = forest_db::rocks::RocksDb::open(db_path(&config), &config.rocks_db)
        .expect("Opening RocksDB must succeed");

    // Initialize ChainStore
    let chain_store = Arc::new(ChainStore::new(db.clone()));

    let publisher = chain_store.publisher();

    // Read Genesis file
    // * When snapshot command implemented, this genesis does not need to be initialized
    let genesis = read_genesis_header(
        config.client.genesis_file.as_ref(),
        config.chain.genesis_bytes(),
        &chain_store,
    )
    .await
    .unwrap();
    chain_store.set_genesis(&genesis.blocks()[0]).unwrap();

    // Reward calculation is needed by the VM to calculate state, which can happen essentially anywhere the `StateManager` is called.
    // It is consensus specific, but threading it through the type system would be a nightmare, which is why dynamic dispatch is used.
    let reward_calc = cns::reward_calc();

    // Initialize StateManager
    let sm = StateManager::new(
        Arc::clone(&chain_store),
        Arc::clone(&config.chain),
        reward_calc,
    )
    .await
    .unwrap();

    let state_manager = Arc::new(sm);

    let network_name = get_network_name_from_genesis(&genesis, &state_manager)
        .await
        .unwrap();

    info!("Using network :: {}", network_name);

    select! {
        () = sync_from_snapshot(&config, &state_manager).fuse() => {},
        _ = ctrlc_oneshot => {
            return;
        },
    }

    // Terminate if no snapshot is provided or DB isn't recent enough
    match chain_store.heaviest_tipset().await {
        None => {
            cli_error_and_die(
                "Forest cannot sync without a snapshot. Download a snapshot from a trusted source and import with --import-snapshot=[file]",
                1,
            );
        }
        Some(tipset) => {
            let epoch = tipset.epoch();
            let nv = config.chain.network_version(epoch);
            if nv < NetworkVersion::V16 {
                cli_error_and_die(
                    "Database too old. Download a snapshot from a trusted source and import with --import-snapshot=[file]",
                    1,
                );
            }
        }
    }

    // Halt
    if config.client.halt_after_import {
        info!("Forest finish shutdown");
        return;
    }

    // Fetch and ensure verification keys are downloaded
    if cns::FETCH_PARAMS {
        use forest_paramfetch::{
            get_params_default, set_proofs_parameter_cache_dir_env, SectorSizeOpt,
        };
        set_proofs_parameter_cache_dir_env(&config.client.data_dir);

        get_params_default(&config.client.data_dir, SectorSizeOpt::Keys, false)
            .await
            .unwrap();
    }

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

    let (tipset_sink, tipset_stream) = bounded(20);

    // Initialize mpool
    let provider = MpoolRpcProvider::new(publisher.clone(), Arc::clone(&state_manager));
    let (mpool, head_changes_task, republish_task) = MessagePool::with_tasks(
        provider,
        network_name.clone(),
        network_send.clone(),
        MpoolConfig::load_config(&db).unwrap(),
        Arc::clone(state_manager.chain_config()),
    )
    .await
    .unwrap();
    let mpool = Arc::new(mpool);

    // For consensus types that do mining, create a component to submit their proposals.
    let submitter = SyncGossipSubmitter::new(
        network_name.clone(),
        network_send.clone(),
        tipset_sink.clone(),
    );

    // Initialize Consensus. Mining may or may not happen, depending on type.
    let (consensus, mining_tasks) = cns::consensus(&state_manager, &keystore, &mpool, submitter)
        .await
        .unwrap();

    // Initialize ChainMuxer
    let chain_muxer_tipset_sink = tipset_sink.clone();
    let chain_muxer = ChainMuxer::new(
        Arc::new(consensus),
        Arc::clone(&state_manager),
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
    let rpc_task = if config.client.enable_rpc {
        let keystore_rpc = Arc::clone(&keystore);
        let rpc_listen = TcpListener::bind(&config.client.rpc_address)
            .await
            .unwrap_or_else(|_| cli_error_and_die("could not bind to {rpc_address}", 1));

        Some(task::spawn(async move {
            info!("JSON-RPC endpoint started at {}", config.client.rpc_address);
            start_rpc::<_, _, FullVerifier, cns::FullConsensus>(
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
                rpc_listen,
                FOREST_VERSION_STRING.as_str(),
            )
            .await
        }))
    } else {
        debug!("RPC disabled.");
        None
    };

    let db_weak_ref = Arc::downgrade(&db.db);
    drop(db);

    // Block until ctrl-c is hit
    ctrlc_oneshot.await.unwrap();

    let keystore_write = task::spawn(async move {
        keystore.read().await.flush().unwrap();
    });

    // Cancel all async services
    for mining_task in mining_tasks {
        mining_task.cancel().await;
    }
    prometheus_server_task.cancel().await;
    head_changes_task.cancel().await;
    republish_task.cancel().await;
    sync_task.cancel().await;
    p2p_task.cancel().await;
    maybe_cancel(rpc_task).await;
    keystore_write.await;

    if db_weak_ref.strong_count() != 0 {
        error!(
            "Dangling reference to DB detected: {}. Please report this as a bug at https://github.com/ChainSafe/forest/issues",
            db_weak_ref.strong_count()
        );
    }
    info!("Forest finish shutdown");
}

async fn sync_from_snapshot(config: &Config, state_manager: &Arc<StateManager<RocksDb>>) {
    if let Some(path) = &config.client.snapshot_path {
        let stopwatch = time::Instant::now();
        let validate_height = if config.client.snapshot {
            config.client.snapshot_height
        } else {
            Some(0)
        };

        import_chain::<FullVerifier, _>(
            state_manager,
            path,
            validate_height,
            config.client.skip_load,
        )
        .await
        .expect("Failed miserably while importing chain from snapshot");
        info!("Imported snapshot in: {}s", stopwatch.elapsed().as_secs());
    }
}

async fn maybe_cancel<R>(mt: Option<JoinHandle<R>>) {
    if let Some(t) = mt {
        t.cancel().await;
    }
}

fn db_path(config: &Config) -> PathBuf {
    chain_path(config).join("db")
}

fn chain_path(config: &Config) -> PathBuf {
    PathBuf::from(&config.client.data_dir).join(&config.chain.name)
}

#[cfg(test)]
mod test {
    use super::*;
    use forest_blocks::BlockHeader;
    use forest_db::MemoryDB;
    use forest_networks::ChainConfig;
    use fvm_shared::address::Address;

    #[async_std::test]
    async fn import_snapshot_from_file() {
        let db = MemoryDB::default();
        let cs = Arc::new(ChainStore::new(db));
        let genesis_header = BlockHeader::builder()
            .miner_address(Address::new_id(0))
            .timestamp(7777)
            .build()
            .unwrap();
        cs.set_genesis(&genesis_header).unwrap();
        let chain_config = Arc::new(ChainConfig::default());
        let sm = Arc::new(
            StateManager::new(
                cs,
                chain_config,
                Arc::new(forest_interpreter::RewardActorMessageCalc),
            )
            .await
            .unwrap(),
        );
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
