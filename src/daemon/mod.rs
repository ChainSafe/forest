// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod bundle;
pub mod main;

use crate::auth::{create_token, generate_priv_key, ADMIN, JWT_IDENTIFIER};
use crate::blocks::Tipset;
use crate::chain::ChainStore;
use crate::chain_sync::{consensus::SyncGossipSubmitter, ChainMuxer};
use crate::cli_shared::{
    chain_path,
    cli::{CliOpts, Config},
    snapshot,
};
use crate::db::{
    db_engine::{db_root, open_proxy_db},
    rolling::DbGarbageCollector,
};
use crate::genesis::{get_network_name_from_genesis, import_chain, read_genesis_header};
use crate::key_management::{
    KeyStore, KeyStoreConfig, ENCRYPTED_KEYSTORE_NAME, FOREST_KEYSTORE_PHRASE_ENV,
};
use crate::libp2p::{Libp2pConfig, Libp2pService, PeerId, PeerManager};
use crate::message_pool::{MessagePool, MpoolConfig, MpoolRpcProvider};
use crate::rpc::start_rpc;
use crate::rpc_api::data_types::RPCState;
use crate::shim::{
    address::{CurrentNetwork, Network},
    clock::ChainEpoch,
    version::NetworkVersion,
};
use crate::state_manager::StateManager;
use crate::utils::{
    monitoring::MemStatsTracker, proofs_api::paramfetch::ensure_params_downloaded, retry,
    version::FOREST_VERSION_STRING, RetryArgs,
};
use anyhow::{bail, Context};
use bundle::load_actor_bundles;
use dialoguer::{console::Term, theme::ColorfulTheme};
use futures::{select, Future, FutureExt};
use lazy_static::lazy_static;
use raw_sync::events::{Event, EventInit as _, EventState};
use shared_memory::ShmemConf;
use std::{
    cell::RefCell,
    net::TcpListener,
    path::{Path, PathBuf},
    sync::Arc,
    time,
    time::Duration,
};
use tempfile::{Builder, TempPath};
use tokio::{
    signal::{
        ctrl_c,
        unix::{signal, SignalKind},
    },
    sync::{mpsc, RwLock},
    task::JoinSet,
};
use tracing::{debug, info, warn};

lazy_static! {
    static ref IPC_PATH: TempPath = Builder::new()
        .prefix("forest-ipc")
        .tempfile()
        .expect("tempfile must succeed")
        .into_temp_path();
}

// The parent process and the daemonized child communicate through an Event in
// shared memory. The identity of the shared memory object is written to a
// temporary file. The parent process is responsible for cleaning up the file
// and the shared memory object.
pub fn ipc_shmem_conf() -> ShmemConf {
    ShmemConf::new()
        .size(Event::size_of(None))
        .force_create_flink()
        .flink(IPC_PATH.as_os_str())
}

use crate::fil_cns::composition as cns;

fn unblock_parent_process() -> anyhow::Result<()> {
    let shmem = ipc_shmem_conf().open()?;
    let (event, _) =
        unsafe { Event::from_existing(shmem.as_ptr()).map_err(|err| anyhow::anyhow!("{err}")) }?;

    event
        .set(EventState::Signaled)
        .map_err(|err| anyhow::anyhow!("{err}"))
}

// Start the daemon and abort if we're interrupted by ctrl-c, SIGTERM, or `forest-cli shutdown`.
pub async fn start_interruptable(opts: CliOpts, config: Config) -> anyhow::Result<()> {
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
    crate::utils::io::terminal_cleanup();
    result
}

/// Starts daemon process
pub(super) async fn start(
    opts: CliOpts,
    config: Config,
    shutdown_send: mpsc::Sender<()>,
) -> anyhow::Result<()> {
    if config.chain.is_testnet() {
        CurrentNetwork::set_global(Network::Testnet);
    }

    info!(
        "Starting Forest daemon, version {}",
        FOREST_VERSION_STRING.as_str()
    );

    let start_time = chrono::Utc::now();
    let path: PathBuf = config.client.data_dir.join("libp2p");
    let net_keypair = crate::libp2p::keypair::get_or_create_keypair(&path)?;

    // Hint at the multihash which has to go in the `/p2p/<multihash>` part of the
    // peer's multiaddress. Useful if others want to use this node to bootstrap
    // from.
    info!("PeerId: {}", PeerId::from(net_keypair.public()));

    let mut keystore = load_or_create_keystore(&config).await?;

    if keystore.get(JWT_IDENTIFIER).is_err() {
        keystore.put(JWT_IDENTIFIER.to_owned(), generate_priv_key())?;
    }

    handle_admin_token(&opts, &config, &keystore)?;

    let keystore = Arc::new(RwLock::new(keystore));

    let chain_data_path = chain_path(&config);
    let db = Arc::new(open_proxy_db(
        db_root(&chain_data_path),
        config.db_config().clone(),
    )?);

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
        let db_directory = crate::db::db_engine::db_root(&chain_path(&config));
        let db = db.clone();
        services.spawn(async {
            crate::metrics::init_prometheus(prometheus_listener, db_directory, db)
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
        Arc::clone(&db),
        db.clone(),
        config.chain.clone(),
        genesis_header.clone(),
    )?);

    let db_garbage_collector = {
        let db = db.clone();
        let chain_store = chain_store.clone();
        let get_tipset = move || chain_store.heaviest_tipset().as_ref().clone();
        Arc::new(DbGarbageCollector::new(
            db.as_ref().clone(),
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

    // Initialize StateManager
    let sm = StateManager::new(Arc::clone(&chain_store), Arc::clone(&config.chain))?;

    let state_manager = Arc::new(sm);

    let network_name = get_network_name_from_genesis(&genesis_header, &state_manager)?;

    info!("Using network :: {}", get_actual_chain_name(&network_name));

    let (tipset_sink, tipset_stream) = flume::bounded(20);

    // if bootstrap peers are not set, set them
    let config = if config.network.bootstrap_peers.is_empty() {
        let bootstrap_peers = config.chain.bootstrap_peers.clone();

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

    let epoch = chain_store.heaviest_tipset().epoch();

    load_actor_bundles(&db).await?;

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
        MpoolConfig::load_config(db.as_ref())?,
        state_manager.chain_config(),
        &mut services,
    )?;

    let mpool = Arc::new(mpool);

    // For consensus types that do mining, create a component to submit their
    // proposals.
    let submitter = SyncGossipSubmitter::new();

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
            let beacon = Arc::new(
                rpc_state_manager
                    .chain_config()
                    .get_beacon_schedule(chain_store.genesis().timestamp())
                    .into_dyn(),
            );
            start_rpc(
                Arc::new(RPCState {
                    state_manager: Arc::clone(&rpc_state_manager),
                    keystore: keystore_rpc,
                    mpool,
                    bad_blocks,
                    sync_state,
                    network_send,
                    network_name,
                    start_time,
                    // TODO: the RPCState can fetch this itself from the StateManager
                    beacon,
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
        crate::utils::proofs_api::paramfetch::set_proofs_parameter_cache_dir_env(
            &config.client.data_dir,
        );
    }

    let mut config = config;
    fetch_snapshot_if_required(&mut config, epoch, opts.auto_download_snapshot).await?;

    if let Some(path) = &config.client.snapshot_path {
        let stopwatch = time::Instant::now();
        import_chain::<_>(
            &state_manager,
            &path.display().to_string(),
            config.client.skip_load,
            config.client.chunk_size,
            config.client.buffer_size,
        )
        .await
        .context("Failed miserably while importing chain from snapshot")?;
        info!("Imported snapshot in: {}s", stopwatch.elapsed().as_secs());
    }

    if let (true, Some(validate_from)) = (config.client.snapshot, config.client.snapshot_height) {
        // We've been provided a snapshot and asked to validate it
        ensure_params_downloaded().await?;
        // Use the specified HEAD, otherwise take the current HEAD.
        let current_height = config
            .client
            .snapshot_head
            .unwrap_or(state_manager.chain_store().heaviest_tipset().epoch());
        assert!(current_height.is_positive());
        match validate_from.is_negative() {
            // allow --height=-1000 to scroll back from the current head
            true => {
                state_manager.validate_range((current_height + validate_from)..=current_height)?
            }
            false => state_manager.validate_range(validate_from..=current_height)?,
        }
    }

    // Halt
    if opts.halt_after_import {
        // Cancel all async services
        services.shutdown().await;
        return Ok(());
    }

    ensure_params_downloaded().await?;
    services.spawn(p2p_service.run());

    // blocking until any of the services returns an error,
    propagate_error(&mut services)
        .await
        .context("services failure")
        .map(|_| {})
}

/// If our current chain is below a supported height, we need a snapshot to bring it up
/// to a supported height. If we've not been given a snapshot by the user, get one.
///
/// An [`Err`] should be considered fatal.
async fn fetch_snapshot_if_required(
    config: &mut Config,
    epoch: ChainEpoch,
    auto_download_snapshot: bool,
) -> anyhow::Result<()> {
    let vendor = snapshot::TrustedVendor::default();
    let path = Path::new(".");
    let chain = &config.chain.network;

    // What height is our chain at right now, and what network version does that correspond to?
    let network_version = config.chain.network_version(epoch);
    let network_version_is_small = network_version < NetworkVersion::V16;

    // We don't support small network versions (we can't validate from e.g genesis).
    // So we need a snapshot (which will be from a recent network version)
    let require_a_snapshot = network_version_is_small;
    let have_a_snapshot = config.client.snapshot_path.is_some();

    match (require_a_snapshot, have_a_snapshot, auto_download_snapshot) {
        (false, _, _) => Ok(()),   // noop - don't need a snapshot
        (true, true, _) => Ok(()), // noop - we need a snapshot, and we have one
        (true, false, true) => {
            // we need a snapshot, don't have one, and have permission to download one, so do that
            let max_retries = 3;
            match retry(
                RetryArgs {
                    timeout: None,
                    max_retries: Some(max_retries),
                    delay: Some(Duration::from_secs(60)),
                },
                || crate::cli_shared::snapshot::fetch(path, chain, vendor),
            )
            .await
            {
                Ok(path) => {
                    config.client.snapshot_path = Some(path);
                    config.client.snapshot = true;
                    Ok(())
                }
                Err(_) => bail!("failed to fetch snapshot after {max_retries} attempts"),
            }
        }
        (true, false, false) => {
            // we need a snapshot, don't have one, and don't have permission to download one, so ask the user
            let (num_bytes, _url) =
                crate::cli_shared::snapshot::peek(vendor, &config.chain.network)
                    .await
                    .context("couldn't get snapshot size")?;
            // dialoguer will double-print long lines, so manually print the first clause ourselves,
            // then let `Confirm` handle the second.
            println!("Forest requires a snapshot to sync with the network, but automatic fetching is disabled.");
            let message = format!(
                "Fetch a {} snapshot to the current directory? (denying will exit the program). ",
                indicatif::HumanBytes(num_bytes)
            );
            let have_permission = asyncify(|| {
                dialoguer::Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt(message)
                    .default(false)
                    .interact()
                    // e.g not a tty (or some other error), so haven't got permission.
                    .unwrap_or(false)
            })
            .await;
            if !have_permission {
                bail!("Forest requires a snapshot to sync with the network, but automatic fetching is disabled.")
            }
            match crate::cli_shared::snapshot::fetch(path, chain, vendor).await {
                Ok(path) => {
                    config.client.snapshot_path = Some(path);
                    config.client.snapshot = true;
                    Ok(())
                }
                Err(e) => Err(e).context("downloading required snapshot failed"),
            }
        }
    }
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

/// returns the first error with which any of the services end, or never returns at all
// This should return anyhow::Result<!> once the `Never` type is stabilized
async fn propagate_error(
    services: &mut JoinSet<Result<(), anyhow::Error>>,
) -> anyhow::Result<std::convert::Infallible> {
    while !services.is_empty() {
        select! {
            option = services.join_next().fuse() => {
                if let Some(Ok(Err(error_message))) = option {
                    return Err(error_message)
                }
            },
        }
    }
    std::future::pending().await
}

fn get_actual_chain_name(internal_network_name: &str) -> &str {
    match internal_network_name {
        "testnetnet" => "mainnet",
        "calibrationnet" => "calibnet",
        _ => internal_network_name,
    }
}

/// This may:
/// - create a [`KeyStore`]
/// - load a [`KeyStore`]
/// - ask a user for password input
async fn load_or_create_keystore(config: &Config) -> anyhow::Result<KeyStore> {
    use std::env::VarError;

    let passphrase_from_env = std::env::var(FOREST_KEYSTORE_PHRASE_ENV);
    let require_encryption = config.client.encrypt_keystore;
    let keystore_already_exists = config
        .client
        .data_dir
        .join(ENCRYPTED_KEYSTORE_NAME)
        .is_dir();

    match (require_encryption, passphrase_from_env) {
        // don't need encryption, we can implicitly create a keystore
        (false, maybe_passphrase) => {
            warn!("Forest has encryption disabled");
            if let Ok(_) | Err(VarError::NotUnicode(_)) = maybe_passphrase {
                warn!(
                    "Ignoring passphrase provided in {} - encryption is disabled",
                    FOREST_KEYSTORE_PHRASE_ENV
                )
            }
            KeyStore::new(KeyStoreConfig::Persistent(config.client.data_dir.clone()))
                .map_err(anyhow::Error::new)
        }

        // need encryption, the user has provided the password through env
        (true, Ok(passphrase)) => KeyStore::new(KeyStoreConfig::Encrypted(
            config.client.data_dir.clone(),
            passphrase,
        ))
        .map_err(anyhow::Error::new),

        // need encryption, we've not been given a password
        (true, Err(error)) => {
            // prompt for passphrase and try and load the keystore

            if let VarError::NotUnicode(_) = error {
                // If we're ignoring the user's password, tell them why
                warn!(
                    "Ignoring passphrase provided in {} - it's not utf-8",
                    FOREST_KEYSTORE_PHRASE_ENV
                )
            }

            let data_dir = config.client.data_dir.clone();

            match keystore_already_exists {
                true => asyncify(move || input_password_to_load_encrypted_keystore(data_dir))
                    .await
                    .context("Couldn't load keystore"),
                false => {
                    let password =
                        asyncify(|| create_password("Create a password for Forest's keystore"))
                            .await?;
                    KeyStore::new(KeyStoreConfig::Encrypted(data_dir, password))
                        .context("Couldn't create keystore")
                }
            }
        }
    }
}

/// Run the closure on a thread where blocking is allowed
///
/// # Panics
/// If the closure panics
fn asyncify<T>(f: impl FnOnce() -> T + Send + 'static) -> impl Future<Output = T>
where
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f).then(|res| async { res.expect("spawned task panicked") })
}

/// Prompts for password, looping until the [`KeyStore`] is successfully loaded.
///
/// This code makes blocking syscalls.
fn input_password_to_load_encrypted_keystore(data_dir: PathBuf) -> std::io::Result<KeyStore> {
    let keystore = RefCell::new(None);
    let term = Term::stderr();

    // Unlike `dialoguer::Confirm`, `dialoguer::Password` doesn't fail if the terminal is not a tty
    // so do that check ourselves.
    // This means users can't pipe their password from stdin.
    if !term.is_term() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotConnected,
            "cannot read password from non-terminal",
        ));
    }

    dialoguer::Password::new()
        .with_prompt("Enter the password for Forest's keystore")
        .allow_empty_password(true) // let validator do validation
        .validate_with(|input: &String| {
            KeyStore::new(KeyStoreConfig::Encrypted(data_dir.clone(), input.clone()))
                .map(|created| *keystore.borrow_mut() = Some(created))
                .context(
                    "Error: couldn't load keystore with this password. Try again or press Ctrl+C to abort.",
                )
        })
        .interact_on(&term)?;

    Ok(keystore
        .into_inner()
        .expect("validation succeeded, so keystore must be emplaced"))
}

/// Loops until the user provides two matching passwords.
///
/// This code makes blocking syscalls
fn create_password(prompt: &str) -> std::io::Result<String> {
    let term = Term::stderr();

    // Unlike `dialoguer::Confirm`, `dialoguer::Password` doesn't fail if the terminal is not a tty
    // so do that check ourselves.
    // This means users can't pipe their password from stdin.
    if !term.is_term() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotConnected,
            "cannot read password from non-terminal",
        ));
    }
    dialoguer::Password::new()
        .with_prompt(prompt)
        .allow_empty_password(false)
        .with_confirmation(
            "Confirm password",
            "Error: the passwords do not match. Try again or press Ctrl+C to abort.",
        )
        .interact_on(&term)
}

#[cfg(test)]
mod test {
    use crate::blocks::BlockHeader;
    use crate::cli_shared::cli::{BufferSize, ChunkSize};
    use crate::db::MemoryDB;
    use crate::networks::ChainConfig;
    use crate::shim::address::Address;

    use super::*;

    #[tokio::test]
    async fn import_snapshot_from_file_valid() {
        import_snapshot_from_file("test-snapshots/chain4.car")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn import_snapshot_from_compressed_file_valid() {
        import_snapshot_from_file("test-snapshots/chain4.car.zst")
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn import_snapshot_from_file_invalid() {
        import_snapshot_from_file("Cargo.toml").await.unwrap_err();
    }

    #[tokio::test]
    async fn import_snapshot_from_file_not_found() {
        import_snapshot_from_file("dummy.car").await.unwrap_err();
    }

    #[tokio::test]
    async fn import_snapshot_from_url_not_found() {
        import_snapshot_from_file("https://dummy.com/dummy.car")
            .await
            .unwrap_err();
    }

    async fn import_snapshot_from_file(file_path: &str) -> anyhow::Result<()> {
        let db = Arc::new(MemoryDB::default());
        let chain_config = Arc::new(ChainConfig::default());

        let genesis_header = BlockHeader::builder()
            .miner_address(Address::new_id(0))
            .timestamp(7777)
            .build()?;

        let cs = Arc::new(ChainStore::new(
            db.clone(),
            db,
            chain_config.clone(),
            genesis_header,
        )?);
        let sm = Arc::new(StateManager::new(cs, chain_config)?);
        import_chain::<_>(
            &sm,
            file_path,
            false,
            ChunkSize::default(),
            BufferSize::default(),
        )
        .await?;
        Ok(())
    }

    #[tokio::test]
    async fn import_chain_from_file() -> anyhow::Result<()> {
        let db = Arc::new(MemoryDB::default());
        let chain_config = Arc::new(ChainConfig::default());
        let genesis_header = BlockHeader::builder()
            .miner_address(Address::new_id(0))
            .timestamp(7777)
            .build()?;

        let cs = Arc::new(ChainStore::new(
            db.clone(),
            db,
            chain_config.clone(),
            genesis_header,
        )?);
        let sm = Arc::new(StateManager::new(cs, chain_config)?);
        import_chain::<_>(
            &sm,
            "test-snapshots/chain4.car",
            false,
            ChunkSize::default(),
            BufferSize::default(),
        )
        .await
        .context("Failed to import chain")?;

        Ok(())
    }
}
