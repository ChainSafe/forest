// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod bundle;
pub mod db_util;
pub mod main;

use crate::auth::{create_token, generate_priv_key, ADMIN, JWT_IDENTIFIER};
use crate::blocks::Tipset;
use crate::chain::ChainStore;
use crate::chain_sync::ChainMuxer;
use crate::cli_shared::{car_db_path, snapshot};
use crate::cli_shared::{
    chain_path,
    cli::{CliOpts, Config},
};

use crate::daemon::db_util::{
    import_chain_as_forest_car, load_all_forest_cars, populate_eth_mappings,
};
use crate::db::car::ManyCar;
use crate::db::db_engine::{db_root, open_db};
use crate::db::parity_db::ParityDb;
use crate::db::{ttl::EthMappingCollector, MarkAndSweep, MemoryDB, SettingsExt, CAR_DB_DIR_NAME};
use crate::genesis::read_genesis_header;
use crate::key_management::{
    KeyStore, KeyStoreConfig, ENCRYPTED_KEYSTORE_NAME, FOREST_KEYSTORE_PHRASE_ENV,
};
use crate::libp2p::{Libp2pService, PeerManager};
use crate::message_pool::{MessagePool, MpoolConfig, MpoolRpcProvider};
use crate::networks::{self, ChainConfig};
use crate::rpc::eth::filter::EthEventHandler;
use crate::rpc::start_rpc;
use crate::rpc::RPCState;
use crate::shim::address::{CurrentNetwork, Network};
use crate::shim::clock::ChainEpoch;
use crate::shim::version::NetworkVersion;
use crate::state_manager::StateManager;
use crate::utils;
use crate::utils::{
    monitoring::MemStatsTracker, proofs_api::ensure_proof_params_downloaded,
    version::FOREST_VERSION_STRING,
};
use anyhow::{bail, Context as _};
use bundle::load_actor_bundles;
use dialoguer::console::Term;
use dialoguer::theme::ColorfulTheme;
use futures::{select, Future, FutureExt};
use fvm_ipld_blockstore::Blockstore;
use libp2p::identity::Keypair;
use libp2p::PeerId;
use once_cell::sync::Lazy;
use raw_sync_2::events::{Event, EventInit as _, EventState};
use shared_memory::ShmemConf;
use std::path::Path;
use std::time::Duration;
use std::{cell::RefCell, cmp, path::PathBuf, sync::Arc};
use tempfile::{Builder, TempPath};
use tokio::{
    net::TcpListener,
    signal::{
        ctrl_c,
        unix::{signal, SignalKind},
    },
    sync::{mpsc, RwLock},
    task::JoinSet,
};
use tracing::{debug, info, warn};

static IPC_PATH: Lazy<TempPath> = Lazy::new(|| {
    Builder::new()
        .prefix("forest-ipc")
        .tempfile()
        .expect("tempfile must succeed")
        .into_temp_path()
});

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

fn unblock_parent_process() -> anyhow::Result<()> {
    let shmem = ipc_shmem_conf().open()?;
    let (event, _) =
        unsafe { Event::from_existing(shmem.as_ptr()).map_err(|err| anyhow::anyhow!("{err}")) }?;

    event
        .set(EventState::Signaled)
        .map_err(|err| anyhow::anyhow!("{err}"))
}

/// Increase the file descriptor limit to a reasonable number.
/// This prevents the node from failing if the default soft limit is too low.
/// Note that the value is only increased, never decreased.
fn maybe_increase_fd_limit() -> anyhow::Result<()> {
    static DESIRED_SOFT_LIMIT: u64 = 8192;
    let (soft_before, _) = rlimit::Resource::NOFILE.get()?;

    let soft_after = rlimit::increase_nofile_limit(DESIRED_SOFT_LIMIT)?;
    if soft_before < soft_after {
        debug!("Increased file descriptor limit from {soft_before} to {soft_after}");
    }
    if soft_after < DESIRED_SOFT_LIMIT {
        warn!(
            "File descriptor limit is too low: {soft_after} < {DESIRED_SOFT_LIMIT}. \
            You may encounter 'too many open files' errors.",
        );
    }

    Ok(())
}

// Start the daemon and abort if we're interrupted by ctrl-c, SIGTERM, or `forest-cli shutdown`.
pub async fn start_interruptable(opts: CliOpts, config: Config) -> anyhow::Result<()> {
    let start_time = chrono::Utc::now();
    let mut terminate = signal(SignalKind::terminate())?;
    let (shutdown_send, mut shutdown_recv) = mpsc::channel(1);

    let result = tokio::select! {
        ret = start(start_time, opts, config, shutdown_send) => ret,
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

// Garbage collection interval, currently set at 10 hours.
const GC_INTERVAL: Duration = Duration::from_secs(60 * 60 * 10);

type DbType = ManyCar<Arc<ParityDb>>;

struct DbMetadata {
    db_root_dir: PathBuf,
    forest_car_db_dir: PathBuf,
}

/// This function initialize Forest with below steps
/// - increase file descriptor limit (for parity-db)
/// - setup proofs parameter cache directory
/// - prints Forest version
fn startup_init(opts: &CliOpts, config: &Config) -> anyhow::Result<()> {
    maybe_increase_fd_limit()?;
    // Sets proof parameter file download path early, the files will be checked and
    // downloaded later right after snapshot import step
    crate::utils::proofs_api::set_proofs_parameter_cache_dir_env(&config.client.data_dir);
    info!(
        "Starting Forest daemon, version {}",
        FOREST_VERSION_STRING.as_str()
    );
    if opts.detach {
        tracing::warn!("F3 sidecar is disabled in detach mode");
        std::env::set_var("FOREST_F3_SIDECAR_FFI_ENABLED", "0");
    }
    Ok(())
}

fn get_chain_config_and_set_network(config: &Config) -> Arc<ChainConfig> {
    let chain_config = Arc::new(ChainConfig::from_chain(config.chain()));
    if chain_config.is_testnet() {
        CurrentNetwork::set_global(Network::Testnet);
    }
    chain_config
}

fn get_or_create_p2p_keypair_and_peer_id(config: &Config) -> anyhow::Result<(Keypair, PeerId)> {
    let path = config.client.data_dir.join("libp2p");
    let keypair = crate::libp2p::keypair::get_or_create_keypair(&path)?;
    let peer_id = keypair.public().to_peer_id();
    Ok((keypair, peer_id))
}

async fn load_or_create_keystore_and_configure_jwt(
    opts: &CliOpts,
    config: &Config,
) -> anyhow::Result<(Arc<RwLock<KeyStore>>, String)> {
    let mut keystore = load_or_create_keystore(config).await?;
    if keystore.get(JWT_IDENTIFIER).is_err() {
        keystore.put(JWT_IDENTIFIER, generate_priv_key())?;
    }
    let admin_jwt = handle_admin_token(opts, &keystore)?;
    let keystore = Arc::new(RwLock::new(keystore));
    Ok((keystore, admin_jwt))
}

fn maybe_migrate_db(config: &Config) {
    // Try to migrate the database if needed. In case the migration fails, we fallback to creating a new database
    // to avoid breaking the node.
    let db_migration = crate::db::migration::DbMigration::new(config);
    if let Err(e) = db_migration.migrate() {
        warn!("Failed to migrate database: {e}");
    }
}

/// This function configures database with below steps
/// - migrate database auto-magically on Forest version bump
/// - load parity-db
/// - load CAR database
/// - load actor bundles
async fn setup_db(opts: &CliOpts, config: &Config) -> anyhow::Result<(Arc<DbType>, DbMetadata)> {
    maybe_migrate_db(config);
    let chain_data_path = chain_path(config);
    let db_root_dir = db_root(&chain_data_path)?;
    let db_writer = Arc::new(open_db(db_root_dir.clone(), config.db_config().clone())?);
    let db = Arc::new(ManyCar::new(db_writer.clone()));
    let forest_car_db_dir = db_root_dir.join(CAR_DB_DIR_NAME);
    load_all_forest_cars(&db, &forest_car_db_dir)?;
    if config.client.load_actors && !opts.stateless {
        load_actor_bundles(&db, config.chain()).await?;
    }
    Ok((
        db,
        DbMetadata {
            db_root_dir,
            forest_car_db_dir,
        },
    ))
}

async fn maybe_import_snapshot(
    opts: &CliOpts,
    config: &mut Config,
    db: &DbType,
    db_meta: &DbMetadata,
    state_manager: &Arc<StateManager<DbType>>,
) -> anyhow::Result<()> {
    let chain_config = state_manager.chain_config();
    // Sets the latest snapshot if needed for downloading later
    if config.client.snapshot_path.is_none() && !opts.stateless {
        maybe_set_snapshot_path(
            config,
            chain_config,
            state_manager.chain_store().heaviest_tipset().epoch(),
            opts.auto_download_snapshot,
            &db_meta.db_root_dir,
        )
        .await?;
    }

    // Import chain if needed
    if !opts.skip_load.unwrap_or_default() {
        if let Some(path) = &config.client.snapshot_path {
            let (car_db_path, ts) = import_chain_as_forest_car(
                path,
                &db_meta.forest_car_db_dir,
                config.client.import_mode,
            )
            .await?;
            db.read_only_files(std::iter::once(car_db_path.clone()))?;
            debug!("Loaded car DB at {}", car_db_path.display());
            state_manager
                .chain_store()
                .set_heaviest_tipset(Arc::new(ts.clone()))?;
        }
    }

    if let Some(validate_from) = config.client.snapshot_height {
        // We've been provided a snapshot and asked to validate it
        ensure_proof_params_downloaded().await?;
        // Use the specified HEAD, otherwise take the current HEAD.
        let current_height = config
            .client
            .snapshot_head
            .unwrap_or_else(|| state_manager.chain_store().heaviest_tipset().epoch());
        assert!(current_height.is_positive());
        match validate_from.is_negative() {
            // allow --height=-1000 to scroll back from the current head
            true => {
                state_manager.validate_range((current_height + validate_from)..=current_height)?
            }
            false => state_manager.validate_range(validate_from..=current_height)?,
        }
    }

    Ok(())
}

async fn create_state_manager(
    config: &Config,
    db: &Arc<DbType>,
    chain_config: &Arc<ChainConfig>,
) -> anyhow::Result<Arc<StateManager<DbType>>> {
    // Read Genesis file
    // * When snapshot command implemented, this genesis does not need to be
    //   initialized
    let genesis_header = read_genesis_header(
        config.client.genesis_file.as_deref(),
        chain_config.genesis_bytes(db).await?.as_deref(),
        db,
    )
    .await?;

    let chain_store = Arc::new(ChainStore::new(
        Arc::clone(db),
        Arc::new(db.clone()),
        db.writer().clone(),
        chain_config.clone(),
        genesis_header.clone(),
    )?);

    // Initialize StateManager
    let state_manager = Arc::new(StateManager::new(
        Arc::clone(&chain_store),
        Arc::clone(chain_config),
        Arc::new(config.sync.clone()),
    )?);

    Ok(state_manager)
}

fn maybe_start_track_peak_rss_service(services: &mut JoinSet<anyhow::Result<()>>, opts: &CliOpts) {
    if opts.track_peak_rss {
        let mem_stats_tracker = MemStatsTracker::default();
        services.spawn(async move {
            mem_stats_tracker.run_loop().await;
            Ok(())
        });
    }
}

async fn maybe_start_metrics_service(
    services: &mut JoinSet<anyhow::Result<()>>,
    config: &Config,
    db: &DbType,
    state_manager: &StateManager<DbType>,
) -> anyhow::Result<()> {
    if config.client.enable_metrics_endpoint {
        // Start Prometheus server port
        let prometheus_listener = TcpListener::bind(config.client.metrics_address)
            .await
            .with_context(|| format!("could not bind to {}", config.client.metrics_address))?;
        info!(
            "Prometheus server started at {}",
            config.client.metrics_address
        );
        let db_directory = crate::db::db_engine::db_root(&chain_path(config))?;
        let db = db.writer().clone();
        services.spawn(async {
            crate::metrics::init_prometheus(prometheus_listener, db_directory, db)
                .await
                .context("Failed to initiate prometheus server")
        });

        crate::metrics::default_registry().register_collector(Box::new(
            networks::metrics::NetworkHeightCollector::new(
                state_manager.chain_config().block_delay_secs,
                state_manager.chain_store().genesis_block_header().timestamp,
            ),
        ));
    }
    Ok(())
}

fn maybe_start_gc_service(
    services: &mut JoinSet<anyhow::Result<()>>,
    opts: &CliOpts,
    config: &Config,
    db: &DbType,
    state_manager: &StateManager<DbType>,
) {
    if !opts.no_gc {
        let mut db_garbage_collector = {
            let chain_store = state_manager.chain_store().clone();
            let depth = cmp::max(
                state_manager.chain_config().policy.chain_finality * 2,
                config.sync.recent_state_roots,
            );

            let get_heaviest_tipset = Box::new(move || chain_store.heaviest_tipset());

            MarkAndSweep::new(
                db.writer().clone(),
                get_heaviest_tipset,
                depth,
                Duration::from_secs(state_manager.chain_config().block_delay_secs as u64),
            )
        };

        services.spawn(async move { db_garbage_collector.gc_loop(GC_INTERVAL).await });
    }
}

fn maybe_start_eth_mapping_collection_service(
    services: &mut JoinSet<anyhow::Result<()>>,
    config: &Config,
    state_manager: &StateManager<DbType>,
) {
    if let Some(ttl) = config.client.eth_mapping_ttl {
        let chain_store = state_manager.chain_store().clone();
        let chain_config = state_manager.chain_config().clone();
        services.spawn(async move {
            tracing::info!("Starting collector for eth_mappings");
            let mut collector = EthMappingCollector::new(
                chain_store.db.clone(),
                chain_config.eth_chain_id,
                Duration::from_secs(ttl.into()),
            );
            collector.run().await
        });
    }
}

async fn create_p2p_service(
    services: &mut JoinSet<anyhow::Result<()>>,
    config: &mut Config,
    state_manager: &StateManager<DbType>,
    net_keypair: Keypair,
    network_name: &str,
) -> anyhow::Result<Libp2pService<DbType>> {
    // if bootstrap peers are not set, set them
    if config.network.bootstrap_peers.is_empty() {
        config.network.bootstrap_peers = state_manager.chain_config().bootstrap_peers.clone();
    }

    let peer_manager = Arc::new(PeerManager::default());
    services.spawn(peer_manager.clone().peer_operation_event_loop_task());
    // Libp2p service setup
    let p2p_service = Libp2pService::new(
        config.network.clone(),
        Arc::clone(state_manager.chain_store()),
        peer_manager.clone(),
        net_keypair,
        network_name,
        *state_manager.chain_store().genesis_block_header().cid(),
    )
    .await?;
    Ok(p2p_service)
}

fn create_chain_muxer(
    services: &mut JoinSet<anyhow::Result<()>>,
    opts: &CliOpts,
    db: &DbType,
    state_manager: &Arc<StateManager<DbType>>,
    p2p_service: &Libp2pService<DbType>,
    network_name: String,
) -> anyhow::Result<ChainMuxer<DbType, MpoolRpcProvider<DbType>>> {
    let publisher = state_manager.chain_store().publisher();
    let provider = MpoolRpcProvider::new(publisher.clone(), state_manager.clone());
    let mpool = Arc::new(MessagePool::new(
        provider,
        network_name,
        p2p_service.network_sender().clone(),
        MpoolConfig::load_config(db.writer().as_ref())?,
        state_manager.chain_config().clone(),
        services,
    )?);
    let chain_muxer = ChainMuxer::new(
        state_manager.clone(),
        p2p_service.peer_manager().clone(),
        mpool.clone(),
        p2p_service.network_sender().clone(),
        p2p_service.network_receiver(),
        Arc::new(Tipset::from(
            state_manager.chain_store().genesis_block_header(),
        )),
        opts.stateless,
    )?;
    Ok(chain_muxer)
}

fn start_chain_muxer_service(
    services: &mut JoinSet<anyhow::Result<()>>,
    chain_muxer: ChainMuxer<DbType, MpoolRpcProvider<DbType>>,
) {
    services.spawn(async { Err(anyhow::anyhow!("{}", chain_muxer.await)) });
}

async fn maybe_start_health_check_service(
    services: &mut JoinSet<anyhow::Result<()>>,
    config: &Config,
    state_manager: &StateManager<DbType>,
    p2p_service: &Libp2pService<DbType>,
    chain_muxer: &ChainMuxer<DbType, MpoolRpcProvider<DbType>>,
) -> anyhow::Result<()> {
    if config.client.enable_health_check {
        let forest_state = crate::health::ForestState {
            config: config.clone(),
            chain_config: state_manager.chain_config().clone(),
            genesis_timestamp: state_manager.chain_store().genesis_block_header().timestamp,
            sync_state: chain_muxer.sync_state().clone(),
            peer_manager: p2p_service.peer_manager().clone(),
            settings_store: state_manager.chain_store().settings(),
        };
        let listener =
            tokio::net::TcpListener::bind(forest_state.config.client.healthcheck_address).await?;
        services.spawn(async move {
            crate::health::init_healthcheck_server(forest_state, listener)
                .await
                .context("Failed to initiate healthcheck server")
        });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn maybe_start_rpc_service(
    services: &mut JoinSet<anyhow::Result<()>>,
    config: &Config,
    keystore: Arc<RwLock<KeyStore>>,
    state_manager: &Arc<StateManager<DbType>>,
    chain_muxer: &ChainMuxer<DbType, MpoolRpcProvider<DbType>>,
    start_time: chrono::DateTime<chrono::Utc>,
    network_name: String,
    shutdown: mpsc::Sender<()>,
) -> anyhow::Result<()> {
    if config.client.enable_rpc {
        let rpc_address = config.client.rpc_address;
        let filter_list = config
            .client
            .rpc_filter_list
            .as_ref()
            .map(|path| crate::rpc::FilterList::new_from_file(path))
            .transpose()?;
        info!("JSON-RPC endpoint will listen at {rpc_address}");
        let eth_event_handler = Arc::new(EthEventHandler::from_config(&config.events));
        services.spawn({
            let state_manager = state_manager.clone();
            let mpool = chain_muxer.mpool().clone();
            let bad_blocks = chain_muxer.bad_blocks().clone();
            let sync_state = chain_muxer.sync_state().clone();
            let sync_network_context = chain_muxer.sync_network_context().clone();
            let tipset_send = chain_muxer.tipset_sender().clone();
            async move {
                start_rpc(
                    RPCState {
                        state_manager,
                        keystore,
                        mpool,
                        bad_blocks,
                        sync_state,
                        eth_event_handler,
                        sync_network_context,
                        network_name,
                        start_time,
                        shutdown,
                        tipset_send,
                    },
                    rpc_address,
                    filter_list,
                )
                .await
            }
        });
    } else {
        debug!("RPC disabled.");
    };
    Ok(())
}

fn maybe_start_f3_service(
    services: &mut JoinSet<anyhow::Result<()>>,
    opts: &CliOpts,
    config: &Config,
    state_manager: &Arc<StateManager<DbType>>,
    p2p_peer_id: PeerId,
    admin_jwt: String,
) {
    if !config.client.enable_rpc {
        if crate::f3::is_sidecar_ffi_enabled(state_manager.chain_config()) {
            tracing::warn!("F3 sidecar is enabled but not run because RPC is disabled. ")
        }
        return;
    }

    if !opts.halt_after_import && !opts.stateless {
        let rpc_address = config.client.rpc_address;
        services.spawn_blocking({
            crate::rpc::f3::F3_LEASE_MANAGER
                .set(crate::rpc::f3::F3LeaseManager::new(
                    state_manager.chain_config().network.clone(),
                    p2p_peer_id,
                ))
                .expect("F3 lease manager should not have been initialized before");
            let chain_config = state_manager.chain_config().clone();
            let default_f3_root = config
                .client
                .data_dir
                .join(format!("f3/{}", config.chain()));
            let crate::f3::F3Options {
                chain_finality,
                bootstrap_epoch,
                initial_power_table,
                manifest_server,
            } = crate::f3::get_f3_sidecar_params(&chain_config);
            move || {
                crate::f3::run_f3_sidecar_if_enabled(
                    &chain_config,
                    format!("http://{rpc_address}/rpc/v1"),
                    admin_jwt,
                    crate::rpc::f3::get_f3_rpc_endpoint().to_string(),
                    initial_power_table.to_string(),
                    bootstrap_epoch,
                    chain_finality,
                    std::env::var("FOREST_F3_ROOT")
                        .unwrap_or(default_f3_root.display().to_string()),
                    manifest_server.map(|i| i.to_string()).unwrap_or_default(),
                );
                Ok(())
            }
        });
    }
}

fn maybe_populate_eth_mappings_in_background(
    services: &mut JoinSet<anyhow::Result<()>>,
    opts: &CliOpts,
    config: Config,
    state_manager: &Arc<StateManager<DbType>>,
) {
    if !opts.stateless && !state_manager.chain_config().is_devnet() {
        let state_manager = state_manager.clone();
        services.spawn(async move {
            if let Err(err) = init_ethereum_mapping(state_manager, &config) {
                tracing::warn!("Init Ethereum mapping failed: {}", err)
            }
            Ok(())
        });
    }
}

/// Starts daemon process
pub(super) async fn start(
    start_time: chrono::DateTime<chrono::Utc>,
    opts: CliOpts,
    mut config: Config,
    shutdown_send: mpsc::Sender<()>,
) -> anyhow::Result<()> {
    startup_init(&opts, &config)?;
    let mut services = JoinSet::new();
    maybe_start_track_peak_rss_service(&mut services, &opts);
    let chain_config = get_chain_config_and_set_network(&config);
    let (net_keypair, p2p_peer_id) = get_or_create_p2p_keypair_and_peer_id(&config)?;
    let (keystore, admin_jwt) = load_or_create_keystore_and_configure_jwt(&opts, &config).await?;
    let (db, db_meta) = setup_db(&opts, &config).await?;
    let state_manager = create_state_manager(&config, &db, &chain_config).await?;
    let network_name = state_manager.get_network_name_from_genesis()?;
    info!("Using network :: {}", get_actual_chain_name(&network_name));
    utils::misc::display_chain_logo(config.chain());
    if opts.exit_after_init {
        return Ok(());
    }
    maybe_import_snapshot(&opts, &mut config, &db, &db_meta, &state_manager).await?;
    if opts.halt_after_import {
        // Cancel all async services
        services.shutdown().await;
        return Ok(());
    }
    maybe_start_eth_mapping_collection_service(&mut services, &config, &state_manager);
    maybe_start_metrics_service(&mut services, &config, &db, &state_manager).await?;
    maybe_start_gc_service(&mut services, &opts, &config, &db, &state_manager);
    let p2p_service = create_p2p_service(
        &mut services,
        &mut config,
        &state_manager,
        net_keypair,
        &network_name,
    )
    .await?;
    let chain_muxer = create_chain_muxer(
        &mut services,
        &opts,
        &db,
        &state_manager,
        &p2p_service,
        network_name.clone(),
    )?;
    maybe_start_rpc_service(
        &mut services,
        &config,
        keystore,
        &state_manager,
        &chain_muxer,
        start_time,
        network_name,
        shutdown_send.clone(),
    )?;
    maybe_start_f3_service(
        &mut services,
        &opts,
        &config,
        &state_manager,
        p2p_peer_id,
        admin_jwt,
    );
    maybe_start_health_check_service(
        &mut services,
        &config,
        &state_manager,
        &p2p_service,
        &chain_muxer,
    )
    .await?;
    maybe_populate_eth_mappings_in_background(&mut services, &opts, config, &state_manager);
    if !opts.stateless {
        ensure_proof_params_downloaded().await?;
    }
    services.spawn(p2p_service.run());
    start_chain_muxer_service(&mut services, chain_muxer);
    // Note: it could take long before unblocking parent process on a fresh Forest run which
    // downloads a snapshot, actor bundles and proof parameter files.
    // This could be moved to before any of those steps if we want to unblock parent process early,
    // the downside would be that, if an error occurs, the log can only be be found in files instead
    // of the console output.
    if opts.detach {
        unblock_parent_process()?;
    }
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
async fn maybe_set_snapshot_path(
    config: &mut Config,
    chain_config: &ChainConfig,
    epoch: ChainEpoch,
    auto_download_snapshot: bool,
    download_directory: &Path,
) -> anyhow::Result<()> {
    if !download_directory.is_dir() {
        anyhow::bail!(
            "`download_directory` does not exist: {}",
            download_directory.display()
        );
    }

    let vendor = snapshot::TrustedVendor::default();
    let chain = config.chain();

    // What height is our chain at right now, and what network version does that correspond to?
    let network_version = chain_config.network_version(epoch);
    let network_version_is_small = network_version < NetworkVersion::V16;

    // We don't support small network versions (we can't validate from e.g genesis).
    // So we need a snapshot (which will be from a recent network version)
    let require_a_snapshot = network_version_is_small;
    let have_a_snapshot = config.client.snapshot_path.is_some();

    match (require_a_snapshot, have_a_snapshot, auto_download_snapshot) {
        (false, _, _) => {}   // noop - don't need a snapshot
        (true, true, _) => {} // noop - we need a snapshot, and we have one
        (true, false, true) => {
            let url = crate::cli_shared::snapshot::stable_url(vendor, chain)?;
            config.client.snapshot_path = Some(url.to_string().into());
        }
        (true, false, false) => {
            // we need a snapshot, don't have one, and don't have permission to download one, so ask the user
            let (url, num_bytes, _path) = crate::cli_shared::snapshot::peek(vendor, chain)
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
            config.client.snapshot_path = Some(url.to_string().into());
        }
    };

    Ok(())
}

/// Generates, prints and optionally writes to a file the administrator JWT
/// token.
fn handle_admin_token(opts: &CliOpts, keystore: &KeyStore) -> anyhow::Result<String> {
    let ki = keystore.get(JWT_IDENTIFIER)?;
    // Lotus admin tokens do not expire but Forest requires all JWT tokens to
    // have an expiration date. So we set the expiration date to 100 years in
    // the future to match user-visible behavior of Lotus.
    let token_exp = chrono::Duration::days(365 * 100);
    let token = create_token(
        ADMIN.iter().map(ToString::to_string).collect(),
        ki.private_key(),
        token_exp,
    )?;
    info!("Admin token: {token}");
    if let Some(path) = opts.save_token.as_ref() {
        if let Some(dir) = path.parent() {
            if !dir.is_dir() {
                std::fs::create_dir_all(dir).with_context(|| {
                    format!(
                        "Failed to create `--save-token` directory {}",
                        dir.display()
                    )
                })?;
            }
        }
        std::fs::write(path, &token)
            .with_context(|| format!("Failed to save admin token to {}", path.display()))?;
    }

    Ok(token)
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

pub fn get_actual_chain_name(internal_network_name: &str) -> &str {
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
fn input_password_to_load_encrypted_keystore(data_dir: PathBuf) -> dialoguer::Result<KeyStore> {
    let keystore = RefCell::new(None);
    let term = Term::stderr();

    // Unlike `dialoguer::Confirm`, `dialoguer::Password` doesn't fail if the terminal is not a tty
    // so do that check ourselves.
    // This means users can't pipe their password from stdin.
    if !term.is_term() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotConnected,
            "cannot read password from non-terminal",
        )
        .into());
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
fn create_password(prompt: &str) -> dialoguer::Result<String> {
    let term = Term::stderr();

    // Unlike `dialoguer::Confirm`, `dialoguer::Password` doesn't fail if the terminal is not a tty
    // so do that check ourselves.
    // This means users can't pipe their password from stdin.
    if !term.is_term() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotConnected,
            "cannot read password from non-terminal",
        )
        .into());
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

fn init_ethereum_mapping<DB: Blockstore>(
    state_manager: Arc<StateManager<DB>>,
    config: &Config,
) -> anyhow::Result<()> {
    match state_manager
        .chain_store()
        .settings()
        .eth_mapping_up_to_date()?
    {
        Some(false) | None => {
            let car_db_path = car_db_path(config)?;
            let db: Arc<ManyCar<MemoryDB>> = Arc::default();
            load_all_forest_cars(&db, &car_db_path)?;
            let ts = db.heaviest_tipset()?;

            populate_eth_mappings(&state_manager, &ts)?;

            state_manager
                .chain_store()
                .settings()
                .set_eth_mapping_up_to_date()
        }
        Some(true) => {
            tracing::info!("Ethereum mapping up to date");
            Ok(())
        }
    }
}
