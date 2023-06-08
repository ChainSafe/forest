// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{net::TcpListener, path::PathBuf, sync::Arc, time, time::Duration};

use anyhow::Context;
use dialoguer::{theme::ColorfulTheme, Confirm, Password};
use forest_auth::{create_token, generate_priv_key, ADMIN, JWT_IDENTIFIER};
use forest_blocks::Tipset;
use forest_chain::ChainStore;
use forest_chain_sync::{consensus::SyncGossipSubmitter, ChainMuxer};
use forest_cli_shared::{
    chain_path,
    cli::{
        default_snapshot_dir, is_aria2_installed, snapshot_fetch, snapshot_fetch_size,
        to_size_string, CliOpts, Client, Config, SnapshotServer,
    },
};
use forest_daemon::bundle::load_bundles;
use forest_db::{
    db_engine::{db_root, open_proxy_db},
    rolling::DbGarbageCollector,
    Store,
};
use forest_genesis::{
    get_network_name_from_genesis, import_chain, read_genesis_header, validate_chain,
};
use forest_key_management::{
    KeyStore, KeyStoreConfig, ENCRYPTED_KEYSTORE_NAME, FOREST_KEYSTORE_PHRASE_ENV,
};
use forest_libp2p::{get_keypair, Libp2pConfig, Libp2pService, PeerId, PeerManager};
use forest_message_pool::{MessagePool, MpoolConfig, MpoolRpcProvider};
use forest_rpc::start_rpc;
use forest_rpc_api::data_types::RPCState;
use forest_shim::version::NetworkVersion;
use forest_state_manager::StateManager;
use forest_utils::{
    io::write_to_file, monitoring::MemStatsTracker,
    proofs_api::paramfetch::ensure_params_downloaded, retry, version::FOREST_VERSION_STRING,
};
use futures::{select, FutureExt};
use log::{debug, error, info, warn};
use raw_sync::events::{Event, EventInit, EventState};
use tokio::{
    signal::{
        ctrl_c,
        unix::{signal, SignalKind},
    },
    sync::{mpsc, RwLock},
    task::JoinSet,
    time::sleep,
};

// Initialize Consensus
#[cfg(not(any(feature = "forest_fil_cns", feature = "forest_deleg_cns")))]
compile_error!("No consensus feature enabled; use e.g. `--feature forest_fil_cns` to pick one.");

cfg_if::cfg_if! {
    if #[cfg(feature = "forest_deleg_cns")] {
        // Custom consensus.
        use forest_deleg_cns::composition as cns;
    } else {
        // Default consensus
        use forest_fil_cns::composition as cns;
    }
}

fn unblock_parent_process() -> anyhow::Result<()> {
    let shmem = super::ipc_shmem_conf().open()?;
    let (event, _) =
        unsafe { Event::from_existing(shmem.as_ptr()).map_err(|err| anyhow::anyhow!("{err}")) }?;

    event
        .set(EventState::Signaled)
        .map_err(|err| anyhow::anyhow!("{err}"))
}

// Start the daemon and abort if we're interrupted by ctrl-c, SIGTERM, or `forest-cli shutdown`.
pub(super) async fn start_interruptable(opts: CliOpts, config: Config) -> anyhow::Result<()> {
    let mut terminate = signal(SignalKind::terminate())?;
    let (shutdown_send, mut shutdown_recv) = mpsc::channel(1);

    let result = tokio::select! {
        ret = start(opts, config, shutdown_send) => ret,
        _ = ctrl_c() => {
            info!("Keyboard interrupt.");
            Ok(())
        },
        _ = terminate.recv() => {
            info!("Received SIGTERM.");
            Ok(())
        },
        _ = shutdown_recv.recv() => {
            info!("Client requested a shutdown.");
            Ok(())
        },
    };
    forest_utils::io::terminal_cleanup();
    result
}

/// Starts daemon process
pub(super) async fn start(
    opts: CliOpts,
    config: Config,
    shutdown_send: mpsc::Sender<()>,
) -> anyhow::Result<()> {
    if config.chain.is_testnet() {
        forest_shim::address::set_current_network(forest_shim::address::Network::Testnet);
    }

    info!(
        "Starting Forest daemon, version {}",
        FOREST_VERSION_STRING.as_str()
    );

    let path: PathBuf = config.client.data_dir.join("libp2p");
    let net_keypair = match get_keypair(&path.join("keypair")) {
        Some(keypair) => Ok::<forest_libp2p::Keypair, std::io::Error>(keypair),
        None => {
            let gen_keypair = forest_libp2p::Keypair::generate_ed25519();
            // Save Ed25519 keypair to file
            // TODO rename old file to keypair.old(?)
            let file = write_to_file(
                &gen_keypair
                    .clone()
                    .into_ed25519()
                    .ok_or(anyhow::anyhow!("couldn't convert keypair to ed25519"))?
                    .encode(),
                &path,
                "keypair",
            )?;
            // Restrict permissions on files containing private keys
            forest_utils::io::set_user_perm(&file)?;
            Ok(gen_keypair)
        }
    }?;

    // Hint at the multihash which has to go in the `/p2p/<multihash>` part of the
    // peer's multiaddress. Useful if others want to use this node to bootstrap
    // from.
    info!("PeerId: {}", PeerId::from(net_keypair.public()));

    let mut keystore = create_keystore(&config).await?;

    if keystore.get(JWT_IDENTIFIER).is_err() {
        keystore.put(JWT_IDENTIFIER.to_owned(), generate_priv_key())?;
    }

    handle_admin_token(&opts, &config, &keystore)?;

    let keystore = Arc::new(RwLock::new(keystore));

    let chain_data_path = chain_path(&config);
    let db = open_proxy_db(db_root(&chain_data_path), config.db_config().clone())?;

    let mut services = JoinSet::new();

    if opts.track_peak_rss {
        let mem_stats_tracker = MemStatsTracker::default();
        services.spawn(async move {
            mem_stats_tracker.run_loop().await;
            Ok(())
        });
    }

    {
        // Start Prometheus server port
        let prometheus_listener = TcpListener::bind(config.client.metrics_address).context(
            format!("could not bind to {}", config.client.metrics_address),
        )?;
        info!(
            "Prometheus server started at {}",
            config.client.metrics_address
        );
        let db_directory = forest_db::db_engine::db_root(&chain_path(&config));
        let db = db.clone();
        services.spawn(async {
            forest_metrics::init_prometheus(prometheus_listener, db_directory, db)
                .await
                .context("Failed to initiate prometheus server")
        });
    }

    // Read Genesis file
    // * When snapshot command implemented, this genesis does not need to be
    //   initialized
    let genesis_header = read_genesis_header(
        config.client.genesis_file.as_ref(),
        config.chain.genesis_bytes(),
        &db,
    )
    .await?;

    // Initialize ChainStore
    let chain_store = Arc::new(ChainStore::new(
        db.clone(),
        config.chain.clone(),
        &genesis_header,
        chain_data_path.as_path(),
    )?);

    chain_store.set_genesis(&genesis_header)?;
    let db_garbage_collector = {
        let db = db.clone();
        let file_backed_chain_meta = chain_store.file_backed_chain_meta().clone();
        let chain_store = chain_store.clone();
        let get_tipset = move || chain_store.heaviest_tipset().as_ref().clone();
        Arc::new(DbGarbageCollector::new(
            db,
            file_backed_chain_meta,
            config.chain.policy.chain_finality,
            config.chain.recent_state_roots,
            get_tipset,
        ))
    };

    if !opts.no_gc {
        services.spawn({
            let db_garbage_collector = db_garbage_collector.clone();
            async move { db_garbage_collector.collect_loop_passive().await }
        });
    }
    services.spawn({
        let db_garbage_collector = db_garbage_collector.clone();
        async move { db_garbage_collector.collect_loop_event().await }
    });

    let publisher = chain_store.publisher();

    // Reward calculation is needed by the VM to calculate state, which can happen
    // essentially anywhere the `StateManager` is called. It is consensus
    // specific, but threading it through the type system would be a nightmare,
    // which is why dynamic dispatch is used.
    let reward_calc = cns::reward_calc();

    // Initialize StateManager
    let sm = StateManager::new(
        Arc::clone(&chain_store),
        Arc::clone(&config.chain),
        reward_calc,
    )?;

    let state_manager = Arc::new(sm);

    let network_name = get_network_name_from_genesis(&genesis_header, &state_manager)?;

    info!("Using network :: {}", get_actual_chain_name(&network_name));

    let (tipset_sink, tipset_stream) = flume::bounded(20);

    // if bootstrap peers are not set, set them
    let config = if config.network.bootstrap_peers.is_empty() {
        let bootstrap_peers = config
            .chain
            .bootstrap_peers
            .iter()
            .map(|node| node.parse())
            .collect::<Result<_, _>>()?;
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

    if opts.exit_after_init {
        return Ok(());
    }

    // XXX: This code has to be run before starting the background services.
    //      If it isn't, several threads will be competing for access to stdout.
    // Terminate if no snapshot is provided or DB isn't recent enough

    let epoch = chain_store.heaviest_tipset().epoch();
    let nv = config.chain.network_version(epoch);
    let should_fetch_snapshot = if nv < NetworkVersion::V16 {
        prompt_snapshot_or_die(opts.auto_download_snapshot, &config).await?
    } else {
        false
    };

    load_bundles(epoch, &config, db.clone()).await?;

    let peer_manager = Arc::new(PeerManager::default());
    services.spawn(peer_manager.clone().peer_operation_event_loop_task());
    let genesis_cid = *genesis_header.cid();
    // Libp2p service setup
    let p2p_service = Libp2pService::new(
        config.network.clone(),
        Arc::clone(&chain_store),
        peer_manager.clone(),
        net_keypair,
        &network_name,
        genesis_cid,
    );

    let network_rx = p2p_service.network_receiver();
    let network_send = p2p_service.network_sender();

    // Initialize mpool
    let provider = MpoolRpcProvider::new(publisher.clone(), Arc::clone(&state_manager));
    let mpool = MessagePool::new(
        provider,
        network_name.clone(),
        network_send.clone(),
        MpoolConfig::load_config(&db)?,
        Arc::clone(state_manager.chain_config()),
        &mut services,
    )?;

    let mpool = Arc::new(mpool);

    // For consensus types that do mining, create a component to submit their
    // proposals.
    let submitter = SyncGossipSubmitter::new(
        network_name.clone(),
        network_send.clone(),
        tipset_sink.clone(),
    );

    // Initialize Consensus. Mining may or may not happen, depending on type.
    let consensus =
        cns::consensus(&state_manager, &keystore, &mpool, submitter, &mut services).await?;

    // Initialize ChainMuxer
    let chain_muxer_tipset_sink = tipset_sink.clone();
    let chain_muxer = ChainMuxer::new(
        Arc::new(consensus),
        Arc::clone(&state_manager),
        peer_manager,
        mpool.clone(),
        network_send.clone(),
        network_rx,
        Arc::new(Tipset::from(genesis_header)),
        chain_muxer_tipset_sink,
        tipset_stream,
        config.sync.clone(),
    )?;
    let bad_blocks = chain_muxer.bad_blocks_cloned();
    let sync_state = chain_muxer.sync_state_cloned();
    services.spawn(async { Err(anyhow::anyhow!("{}", chain_muxer.await)) });

    // Start services
    if config.client.enable_rpc {
        let keystore_rpc = Arc::clone(&keystore);
        let rpc_listen =
            std::net::TcpListener::bind(config.client.rpc_address).context(format!(
                "could not bind to rpc address {}",
                config.client.rpc_address
            ))?;

        let rpc_state_manager = Arc::clone(&state_manager);
        let rpc_chain_store = Arc::clone(&chain_store);

        let gc_event_tx = db_garbage_collector.get_tx();
        services.spawn(async move {
            info!("JSON-RPC endpoint started at {}", config.client.rpc_address);
            // XXX: The JSON error message are a nightmare to print.
            start_rpc::<_, _, cns::FullConsensus>(
                Arc::new(RPCState {
                    state_manager: Arc::clone(&rpc_state_manager),
                    keystore: keystore_rpc,
                    mpool,
                    bad_blocks,
                    sync_state,
                    network_send,
                    network_name,
                    // TODO: the RPCState can fetch this itself from the StateManager
                    beacon: rpc_state_manager.beacon_schedule(),
                    chain_store: rpc_chain_store,
                    new_mined_block_tx: tipset_sink,
                    gc_event_tx,
                }),
                rpc_listen,
                FOREST_VERSION_STRING.as_str(),
                shutdown_send,
            )
            .await
            .map_err(|err| anyhow::anyhow!("{:?}", serde_json::to_string(&err)))
        });
    } else {
        debug!("RPC disabled.");
    };
    if opts.detach {
        unblock_parent_process()?;
    }

    // Sets proof parameter file download path early, the files will be checked and
    // downloaded later right after snapshot import step
    if cns::FETCH_PARAMS {
        forest_utils::proofs_api::paramfetch::set_proofs_parameter_cache_dir_env(
            &config.client.data_dir,
        );
    }

    let config = maybe_fetch_snapshot(should_fetch_snapshot, config).await?;

    if let Some(path) = &config.client.snapshot_path {
        let stopwatch = time::Instant::now();
        import_chain::<_>(
            &state_manager,
            &path.display().to_string(),
            config.client.skip_load,
        )
        .await
        .context("Failed miserably while importing chain from snapshot")?;
        info!("Imported snapshot in: {}s", stopwatch.elapsed().as_secs());
    }

    if config.client.snapshot {
        if let Some(validate_height) = config.client.snapshot_height {
            ensure_params_downloaded().await?;
            validate_chain(&state_manager, validate_height).await?;
        }
    }

    // For convenience, flush the database after we've potentially loaded a new
    // snapshot. This ensures the snapshot won't have to be re-imported if
    // Forest is interrupted. As of writing, flushing only affects RocksDB and
    // is a no-op with ParityDB.
    state_manager.blockstore().flush()?;

    // Halt
    if opts.halt_after_import {
        // Cancel all async services
        services.shutdown().await;
        return Ok(());
    }

    ensure_params_downloaded().await?;
    services.spawn(p2p_service.run());

    // blocking until any of the services returns an error,
    let err = propagate_error(&mut services).await;
    anyhow::bail!("services failure: {}", err);
}

/// Generates, prints and optionally writes to a file the administrator JWT
/// token.
fn handle_admin_token(opts: &CliOpts, config: &Config, keystore: &KeyStore) -> anyhow::Result<()> {
    let ki = keystore.get(JWT_IDENTIFIER)?;
    let token_exp = config.client.token_exp;
    let token = create_token(ADMIN.to_owned(), ki.private_key(), token_exp)?;
    info!("Admin token: {token}");
    if let Some(path) = opts.save_token.as_ref() {
        std::fs::write(path, token)?;
    }

    Ok(())
}

// returns the first error with which any of the services end
// in case all services finished without an error sleeps for more than 2 years
// and then returns with an error
async fn propagate_error(services: &mut JoinSet<Result<(), anyhow::Error>>) -> anyhow::Error {
    while !services.is_empty() {
        select! {
            option = services.join_next().fuse() => {
                if let Some(Ok(Err(error_message))) = option {
                    return error_message
                }
            },
        }
    }
    // In case all services are down without errors we are still willing
    // to wait indefinitely for CTRL-C signal. As `tokio::time::sleep` has
    // a limit of approximately 2.2 years we have to loop
    loop {
        tokio::time::sleep(Duration::new(64000000, 0)).await;
    }
}

/// Optionally fetches the snapshot. Returns the configuration (modified
/// accordingly if a snapshot was fetched).
async fn maybe_fetch_snapshot(
    should_fetch_snapshot: bool,
    config: Config,
) -> anyhow::Result<Config> {
    if should_fetch_snapshot {
        let snapshot_path = default_snapshot_dir(&config);
        let provider = SnapshotServer::try_get_default(&config.chain.network)?;
        // FIXME: change this to `true` once zstd compressed snapshots is supported by
        // the forest provider
        let use_compressed = provider == SnapshotServer::Filecoin;
        let path = retry!(
            snapshot_fetch,
            config.daemon.default_retry,
            config.daemon.default_delay,
            &snapshot_path,
            &config,
            &Some(provider),
            use_compressed,
            is_aria2_installed()
        )?;
        Ok(Config {
            client: Client {
                snapshot_path: Some(path),
                snapshot: true,
                ..config.client
            },
            ..config
        })
    } else {
        Ok(config)
    }
}

/// Last resort in case a snapshot is needed. If it is not to be downloaded,
/// this method fails and exits the process.
async fn prompt_snapshot_or_die(
    auto_download_snapshot: bool,
    config: &Config,
) -> anyhow::Result<bool> {
    if config.client.snapshot_path.is_some() {
        return Ok(false);
    }
    let should_download = if !auto_download_snapshot && atty::is(atty::Stream::Stdin) {
        let required_size: u64 = snapshot_fetch_size(config).await?;
        let required_size = to_size_string(&required_size.into())?;
        tokio::task::spawn_blocking(move || Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(
                    format!("Forest needs a snapshot to sync with the network. Would you like to download one now? Required disk space {required_size}."),
                )
                .default(false)
                .interact()
            ).await??
    } else {
        auto_download_snapshot
    };

    if should_download {
        Ok(true)
    } else {
        anyhow::bail!("Forest cannot sync without a snapshot. Download a snapshot from a trusted source and import with --import-snapshot=[file] or --auto-download-snapshot to download one automatically");
    }
}

fn get_actual_chain_name(internal_network_name: &str) -> &str {
    match internal_network_name {
        "testnetnet" => "mainnet",
        "calibrationnet" => "calibnet",
        _ => internal_network_name,
    }
}

async fn create_keystore(config: &Config) -> anyhow::Result<KeyStore> {
    let passphrase = std::env::var(FOREST_KEYSTORE_PHRASE_ENV);
    let is_interactive = atty::is(atty::Stream::Stdin);

    // encrypted keystore, headless
    if config.client.encrypt_keystore && passphrase.is_err() && !is_interactive {
        anyhow::bail!("Passphrase for the keystore was not provided and the encryption was not explicitly disabled. Please set the {FOREST_KEYSTORE_PHRASE_ENV} environmental variable and re-run the command");
    // encrypted keystore, either headless or interactive, passphrase provided
    } else if config.client.encrypt_keystore && passphrase.is_ok() {
        let passphrase = passphrase.unwrap();

        let keystore = KeyStore::new(KeyStoreConfig::Encrypted(
            PathBuf::from(&config.client.data_dir),
            passphrase,
        ));

        keystore.map_err(|_| anyhow::anyhow!("Incorrect passphrase. Please verify the {FOREST_KEYSTORE_PHRASE_ENV} environmental variable."))
    // encrypted keystore, interactive, passphrase not provided
    } else if config.client.encrypt_keystore && passphrase.is_err() && is_interactive {
        loop {
            let passphrase = password_prompt("Enter the keystore passphrase").await?;

            let data_dir = PathBuf::from(&config.client.data_dir).join(ENCRYPTED_KEYSTORE_NAME);
            if !data_dir.exists() {
                let passphrase_again = password_prompt("Confirm passphrase").await?;

                if passphrase != passphrase_again {
                    error!("Passphrases do not match. Please retry.");
                    continue;
                }
            }

            let key_store_init_result = KeyStore::new(KeyStoreConfig::Encrypted(
                config.client.data_dir.clone(),
                passphrase,
            ));

            match key_store_init_result {
                Ok(ks) => break Ok(ks),
                Err(_) => {
                    error!("Incorrect passphrase entered. Please try again.")
                }
            };
        }
    } else {
        warn!("Warning: Keystore encryption disabled!");
        Ok(KeyStore::new(KeyStoreConfig::Persistent(
            config.client.data_dir.clone(),
        ))?)
    }
}

// Prompt for password in a blocking thread such that tokio can still process interrupts.
async fn password_prompt(prompt: impl Into<String>) -> anyhow::Result<String> {
    let prompt: String = prompt.into();
    Ok(tokio::task::spawn_blocking(|| {
        Password::with_theme(&ColorfulTheme::default())
            .allow_empty_password(true)
            .with_prompt(prompt)
            .interact()
    })
    .await??)
}

#[cfg(test)]
mod test {
    use forest_blocks::BlockHeader;
    use forest_db::MemoryDB;
    use forest_networks::ChainConfig;
    use forest_shim::address::Address;
    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn import_snapshot_from_file_valid() -> anyhow::Result<()> {
        anyhow::ensure!(import_snapshot_from_file("test_files/chain4.car")
            .await
            .is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn import_snapshot_from_compressed_file_valid() -> anyhow::Result<()> {
        anyhow::ensure!(import_snapshot_from_file("test_files/chain4.car.zst")
            .await
            .is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn import_snapshot_from_file_invalid() -> anyhow::Result<()> {
        anyhow::ensure!(import_snapshot_from_file("Cargo.toml").await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn import_snapshot_from_file_not_found() -> anyhow::Result<()> {
        anyhow::ensure!(import_snapshot_from_file("dummy.car").await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn import_snapshot_from_url_not_found() -> anyhow::Result<()> {
        anyhow::ensure!(import_snapshot_from_file("https://dummy.com/dummy.car")
            .await
            .is_err());
        Ok(())
    }

    async fn import_snapshot_from_file(file_path: &str) -> anyhow::Result<()> {
        let db = MemoryDB::default();
        let chain_config = Arc::new(ChainConfig::default());

        let genesis_header = BlockHeader::builder()
            .miner_address(Address::new_id(0))
            .timestamp(7777)
            .build()?;

        let chain_data_root = TempDir::new().unwrap();
        let cs = Arc::new(ChainStore::new(
            db,
            chain_config.clone(),
            &genesis_header,
            chain_data_root.path(),
        )?);
        let sm = Arc::new(StateManager::new(
            cs,
            chain_config,
            Arc::new(forest_interpreter::RewardActorMessageCalc),
        )?);
        import_chain::<_>(&sm, file_path, false).await?;
        Ok(())
    }

    #[tokio::test]
    async fn import_chain_from_file() -> anyhow::Result<()> {
        let db = MemoryDB::default();
        let chain_config = Arc::new(ChainConfig::default());
        let genesis_header = BlockHeader::builder()
            .miner_address(Address::new_id(0))
            .timestamp(7777)
            .build()?;

        let chain_data_root = TempDir::new()?;
        let cs = Arc::new(ChainStore::new(
            db,
            chain_config.clone(),
            &genesis_header,
            chain_data_root.path(),
        )?);
        let sm = Arc::new(StateManager::new(
            cs,
            chain_config,
            Arc::new(forest_interpreter::RewardActorMessageCalc),
        )?);
        import_chain::<_>(&sm, "test_files/chain4.car", false)
            .await
            .expect("Failed to import chain");

        Ok(())
    }
}
