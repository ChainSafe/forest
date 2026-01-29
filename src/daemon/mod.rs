// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod bundle;
mod context;
pub mod db_util;
pub mod main;

use crate::blocks::Tipset;
use crate::chain::HeadChange;
use crate::chain::index::ResolveNullTipset;
use crate::chain_sync::network_context::SyncNetworkContext;
use crate::chain_sync::{ChainFollower, SyncStatus};
use crate::cli_shared::snapshot;
use crate::cli_shared::{
    chain_path,
    cli::{CliOpts, Config},
};
use crate::daemon::{
    context::{AppContext, DbType},
    db_util::import_chain_as_forest_car,
};
use crate::db::gc::SnapshotGarbageCollector;
use crate::db::ttl::EthMappingCollector;
use crate::libp2p::{Libp2pService, PeerManager};
use crate::message_pool::{MessagePool, MpoolConfig, MpoolRpcProvider};
use crate::networks::{self, ChainConfig};
use crate::rpc::RPCState;
use crate::rpc::eth::filter::EthEventHandler;
use crate::rpc::start_rpc;
use crate::shim::clock::ChainEpoch;
use crate::shim::state_tree::StateTree;
use crate::shim::version::NetworkVersion;
use crate::utils;
use crate::utils::misc::env::is_env_truthy;
use crate::utils::{proofs_api::ensure_proof_params_downloaded, version::FOREST_VERSION_STRING};
use anyhow::{Context as _, bail};
use dialoguer::theme::ColorfulTheme;
use futures::{Future, FutureExt};
use std::path::Path;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Instant;
use tokio::{
    net::TcpListener,
    signal::{
        ctrl_c,
        unix::{SignalKind, signal},
    },
    sync::mpsc,
    task::JoinSet,
};
use tracing::{debug, info, warn};

pub static GLOBAL_SNAPSHOT_GC: OnceLock<Arc<SnapshotGarbageCollector<DbType>>> = OnceLock::new();

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

/// This function initialize Forest with below steps
/// - increase file descriptor limit (for parity-db)
/// - setup proofs parameter cache directory
/// - prints Forest version
fn startup_init(config: &Config) -> anyhow::Result<()> {
    maybe_increase_fd_limit()?;
    // Sets proof parameter file download path early, the files will be checked and
    // downloaded later right after snapshot import step
    crate::utils::proofs_api::maybe_set_proofs_parameter_cache_dir_env(&config.client.data_dir);
    info!(
        "Starting Forest daemon, version {}",
        FOREST_VERSION_STRING.as_str()
    );
    Ok(())
}

async fn maybe_import_snapshot(
    opts: &CliOpts,
    config: &mut Config,
    ctx: &AppContext,
) -> anyhow::Result<()> {
    let chain_config = ctx.state_manager.chain_config();
    // Sets the latest snapshot if needed for downloading later
    if config.client.snapshot_path.is_none() && !opts.stateless {
        maybe_set_snapshot_path(
            config,
            chain_config,
            ctx.state_manager.chain_store().heaviest_tipset().epoch(),
            opts.auto_download_snapshot,
            &ctx.db_meta_data.get_root_dir(),
        )
        .await?;
    }

    let snapshot_tracker = ctx.snapshot_progress_tracker.clone();
    // Import chain if needed
    if !opts.skip_load.unwrap_or_default()
        && let Some(path) = &config.client.snapshot_path
    {
        let (car_db_path, ts) = import_chain_as_forest_car(
            path,
            &ctx.db_meta_data.get_forest_car_db_dir(),
            config.client.import_mode,
            config.client.rpc_v1_endpoint()?,
            &crate::f3::get_f3_root(config),
            ctx.chain_config(),
            &snapshot_tracker,
        )
        .await?;
        ctx.db
            .read_only_files(std::iter::once(car_db_path.clone()))?;
        let ts_epoch = ts.epoch();
        // Explicitly set heaviest tipset here in case HEAD_KEY has already been set
        // in the current setting store
        ctx.state_manager.chain_store().set_heaviest_tipset(ts)?;
        debug!(
            "Loaded car DB at {} and set current head to epoch {ts_epoch}",
            car_db_path.display(),
        );
    }

    // If the snapshot progress state is not completed,
    // set the state to not required
    if !snapshot_tracker.is_completed() {
        snapshot_tracker.not_required();
    }

    if let Some(validate_from) = config.client.snapshot_height {
        // We've been provided a snapshot and asked to validate it
        ensure_proof_params_downloaded().await?;
        // Use the specified HEAD, otherwise take the current HEAD.
        let current_height = config
            .client
            .snapshot_head
            .unwrap_or_else(|| ctx.state_manager.chain_store().heaviest_tipset().epoch());
        assert!(current_height.is_positive());
        match validate_from.is_negative() {
            // allow --height=-1000 to scroll back from the current head
            true => ctx
                .state_manager
                .validate_range((current_height + validate_from)..=current_height)?,
            false => ctx
                .state_manager
                .validate_range(validate_from..=current_height)?,
        }
    }

    Ok(())
}

async fn maybe_start_metrics_service(
    services: &mut JoinSet<anyhow::Result<()>>,
    config: &Config,
    ctx: &AppContext,
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
        let db = ctx.db.writer().clone();

        let get_chain_head_height = Arc::new({
            // Use `Weak` to not dead lock GC.
            let chain_store = Arc::downgrade(ctx.state_manager.chain_store());
            move || {
                chain_store
                    .upgrade()
                    .map(|cs| cs.heaviest_tipset().epoch())
                    .unwrap_or_default()
            }
        });
        let get_chain_head_actor_version = Arc::new({
            // Use `Weak` to not dead lock GC.
            let chain_store = Arc::downgrade(ctx.state_manager.chain_store());
            move || {
                if let Some(cs) = chain_store.upgrade()
                    && let Ok(state) = StateTree::new_from_root(
                        cs.blockstore().clone(),
                        cs.heaviest_tipset().parent_state(),
                    )
                    && let Ok(bundle_meta) = state.get_actor_bundle_metadata()
                    && let Ok(actor_version) = bundle_meta.actor_major_version()
                {
                    return actor_version;
                }
                0
            }
        });
        services.spawn({
            let chain_config = ctx.chain_config().clone();
            let get_chain_head_height = get_chain_head_height.clone();
            async {
                crate::metrics::init_prometheus(
                    prometheus_listener,
                    db_directory,
                    db,
                    chain_config,
                    get_chain_head_height,
                    get_chain_head_actor_version,
                )
                .await
                .context("Failed to initiate prometheus server")
            }
        });

        crate::metrics::register_collector(Box::new(
            networks::metrics::NetworkHeightCollector::new(
                ctx.state_manager.chain_config().block_delay_secs,
                ctx.state_manager
                    .chain_store()
                    .genesis_block_header()
                    .timestamp,
                get_chain_head_height,
            ),
        ));
    }
    Ok(())
}

async fn create_p2p_service(
    services: &mut JoinSet<anyhow::Result<()>>,
    config: &mut Config,
    ctx: &AppContext,
) -> anyhow::Result<Libp2pService<DbType>> {
    // if bootstrap peers are not set, set them
    if config.network.bootstrap_peers.is_empty() {
        config.network.bootstrap_peers = ctx.state_manager.chain_config().bootstrap_peers.clone();
    }

    let peer_manager = Arc::new(PeerManager::default());
    services.spawn(peer_manager.clone().peer_operation_event_loop_task());
    // Libp2p service setup
    let p2p_service = Libp2pService::new(
        config.network.clone(),
        Arc::clone(ctx.state_manager.chain_store()),
        peer_manager.clone(),
        ctx.net_keypair.clone(),
        config.chain.genesis_name(),
        *ctx.state_manager.chain_store().genesis_block_header().cid(),
    )
    .await?;
    Ok(p2p_service)
}

fn create_mpool(
    services: &mut JoinSet<anyhow::Result<()>>,
    p2p_service: &Libp2pService<DbType>,
    ctx: &AppContext,
) -> anyhow::Result<Arc<MessagePool<MpoolRpcProvider<DbType>>>> {
    let publisher = ctx.state_manager.chain_store().publisher();
    let provider = MpoolRpcProvider::new(publisher.clone(), ctx.state_manager.clone());
    Ok(MessagePool::new(
        provider,
        p2p_service.network_sender().clone(),
        MpoolConfig::load_config(ctx.db.writer().as_ref())?,
        ctx.state_manager.chain_config().clone(),
        services,
    )
    .map(Arc::new)?)
}

fn create_chain_follower(
    opts: &CliOpts,
    p2p_service: &Libp2pService<DbType>,
    mpool: Arc<MessagePool<MpoolRpcProvider<DbType>>>,
    ctx: &AppContext,
) -> anyhow::Result<ChainFollower<DbType>> {
    let network_send = p2p_service.network_sender().clone();
    let peer_manager = p2p_service.peer_manager().clone();
    let network = SyncNetworkContext::new(network_send, peer_manager, ctx.db.clone());
    let chain_follower = ChainFollower::new(
        ctx.state_manager.clone(),
        network,
        Tipset::from(ctx.state_manager.chain_store().genesis_block_header()),
        p2p_service.network_receiver(),
        opts.stateless,
        mpool,
    );
    Ok(chain_follower)
}

fn start_chain_follower_service(
    services: &mut JoinSet<anyhow::Result<()>>,
    chain_follower: ChainFollower<DbType>,
) {
    services.spawn(async move { chain_follower.run().await });
}

async fn maybe_start_health_check_service(
    services: &mut JoinSet<anyhow::Result<()>>,
    config: &Config,
    p2p_service: &Libp2pService<DbType>,
    chain_follower: &ChainFollower<DbType>,
    ctx: &AppContext,
) -> anyhow::Result<()> {
    if config.client.enable_health_check {
        let forest_state = crate::health::ForestState {
            config: config.clone(),
            chain_config: ctx.state_manager.chain_config().clone(),
            genesis_timestamp: ctx
                .state_manager
                .chain_store()
                .genesis_block_header()
                .timestamp,
            sync_status: chain_follower.sync_status.clone(),
            peer_manager: p2p_service.peer_manager().clone(),
        };
        let healthcheck_address = forest_state.config.client.healthcheck_address;
        info!("Healthcheck endpoint will listen at {healthcheck_address}");
        let listener = tokio::net::TcpListener::bind(healthcheck_address).await?;
        services.spawn(async move {
            crate::health::init_healthcheck_server(forest_state, listener)
                .await
                .context("Failed to initiate healthcheck server")
        });
    } else {
        info!("Healthcheck service is disabled");
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn maybe_start_rpc_service(
    services: &mut JoinSet<anyhow::Result<()>>,
    config: &Config,
    mpool: Arc<MessagePool<MpoolRpcProvider<DbType>>>,
    chain_follower: &ChainFollower<DbType>,
    start_time: chrono::DateTime<chrono::Utc>,
    shutdown: mpsc::Sender<()>,
    rpc_stop_handle: jsonrpsee::server::StopHandle,
    ctx: &AppContext,
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
        if is_env_truthy("FOREST_JWT_DISABLE_EXP_VALIDATION") {
            warn!(
                "JWT expiration validation is disabled; this significantly weakens security and should only be used in tightly controlled environments"
            );
        }
        services.spawn({
            let state_manager = ctx.state_manager.clone();
            let bad_blocks = chain_follower.bad_blocks.clone();
            let sync_status = chain_follower.sync_status.clone();
            let sync_network_context = chain_follower.network.clone();
            let tipset_send = chain_follower.tipset_sender.clone();
            let keystore = ctx.keystore.clone();
            let snapshot_progress_tracker = ctx.snapshot_progress_tracker.clone();
            let msgs_in_tipset = Arc::new(crate::chain::MsgsInTipsetCache::default());
            async move {
                let rpc_listener = tokio::net::TcpListener::bind(rpc_address)
                    .await
                    .map_err(|e| {
                        anyhow::anyhow!("Unable to listen on RPC endpoint {rpc_address}: {e}")
                    })
                    .unwrap();
                start_rpc(
                    RPCState {
                        state_manager,
                        keystore,
                        mpool,
                        bad_blocks,
                        msgs_in_tipset,
                        sync_status,
                        eth_event_handler,
                        sync_network_context,
                        start_time,
                        shutdown,
                        tipset_send,
                        snapshot_progress_tracker,
                    },
                    rpc_listener,
                    rpc_stop_handle,
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

fn maybe_start_f3_service(opts: &CliOpts, config: &Config, ctx: &AppContext) -> anyhow::Result<()> {
    // already running
    if crate::rpc::f3::F3_LEASE_MANAGER.get().is_some() {
        return Ok(());
    }

    if !config.client.enable_rpc {
        if crate::f3::is_sidecar_ffi_enabled(ctx.state_manager.chain_config()) {
            tracing::warn!("F3 sidecar is enabled but not run because RPC is disabled. ")
        }
        return Ok(());
    }

    if !opts.halt_after_import && !opts.stateless {
        let rpc_endpoint = config.client.rpc_v1_endpoint()?;
        let state_manager = &ctx.state_manager;
        let p2p_peer_id = ctx.p2p_peer_id;
        let admin_jwt = ctx.admin_jwt.clone();
        tokio::task::spawn_blocking({
            crate::rpc::f3::F3_LEASE_MANAGER
                .set(crate::rpc::f3::F3LeaseManager::new(
                    state_manager.chain_config().network.clone(),
                    p2p_peer_id,
                ))
                .expect("F3 lease manager should not have been initialized before");
            let chain_config = state_manager.chain_config().clone();
            let f3_root = crate::f3::get_f3_root(config);
            let crate::f3::F3Options {
                chain_finality,
                bootstrap_epoch,
                initial_power_table,
            } = crate::f3::get_f3_sidecar_params(&chain_config);
            move || {
                crate::f3::run_f3_sidecar_if_enabled(
                    &chain_config,
                    rpc_endpoint.to_string(),
                    admin_jwt,
                    crate::rpc::f3::get_f3_rpc_endpoint().to_string(),
                    initial_power_table
                        .map(|i| i.to_string())
                        .unwrap_or_default(),
                    bootstrap_epoch,
                    chain_finality,
                    f3_root.display().to_string(),
                );
            }
        });
    }

    Ok(())
}

fn maybe_start_indexer_service(
    services: &mut JoinSet<anyhow::Result<()>>,
    opts: &CliOpts,
    config: &Config,
    ctx: &AppContext,
) {
    if config.chain_indexer.enable_indexer
        && !opts.stateless
        && !ctx.state_manager.chain_config().is_devnet()
    {
        let mut receiver = ctx.state_manager.chain_store().publisher().subscribe();
        let chain_store = ctx.state_manager.chain_store().clone();
        services.spawn(async move {
            tracing::info!("Starting indexer service");

            // Continuously listen for head changes
            loop {
                let msg = receiver.recv().await?;

                let HeadChange::Apply(ts) = msg;
                tracing::debug!("Indexing tipset {}", ts.key());

                chain_store.put_tipset_key(ts.key())?;

                let delegated_messages =
                    chain_store.headers_delegated_messages(ts.block_headers().iter())?;

                chain_store.process_signed_messages(&delegated_messages)?;
            }
        });

        // Run the collector only if chain indexer is enabled
        if let Some(retention_epochs) = config.chain_indexer.gc_retention_epochs {
            let chain_store = ctx.state_manager.chain_store().clone();
            let chain_config = ctx.state_manager.chain_config().clone();
            services.spawn(async move {
                tracing::info!("Starting collector for eth_mappings");
                let mut collector = EthMappingCollector::new(
                    chain_store.blockstore().clone(),
                    chain_config.eth_chain_id,
                    retention_epochs.into(),
                );
                collector.run().await
            });
        }
    }
}

/// Starts daemon process
pub(super) async fn start(
    start_time: chrono::DateTime<chrono::Utc>,
    opts: CliOpts,
    config: Config,
    shutdown_send: mpsc::Sender<()>,
) -> anyhow::Result<()> {
    startup_init(&config)?;
    let (snap_gc, snap_gc_reboot_rx) = SnapshotGarbageCollector::new(&config)?;
    let snap_gc = Arc::new(snap_gc);
    GLOBAL_SNAPSHOT_GC
        .set(snap_gc.clone())
        .ok()
        .context("failed to set GLOBAL_SNAPSHOT_GC")?;

    // If the node is stateless, GC shouldn't get triggered even on demand.
    if !opts.stateless {
        tokio::task::spawn({
            let snap_gc = snap_gc.clone();
            async move { snap_gc.event_loop().await }
        });
    }
    // GC shouldn't run periodically if the node is stateless or if the user has disabled it.
    if !opts.no_gc && !opts.stateless {
        tokio::task::spawn({
            let snap_gc = snap_gc.clone();
            async move { snap_gc.scheduler_loop().await }
        });
    }
    loop {
        let (rpc_stop_handle, rpc_server_handle) = jsonrpsee::server::stop_channel();
        tokio::select! {
            _ = snap_gc_reboot_rx.recv_async() => {
                // gracefully shutdown RPC server
                if let Err(e) = rpc_server_handle.stop() {
                    tracing::warn!("failed to stop RPC server: {e}");
                }
                snap_gc.cleanup_before_reboot().await;
            }
            result = start_services(start_time, &opts, config.clone(), shutdown_send.clone(), rpc_stop_handle, |ctx, sync_status| {
                snap_gc.set_db(ctx.db.clone());
                snap_gc.set_sync_status(sync_status);
                snap_gc.set_car_db_head_epoch(ctx.db.heaviest_tipset().map(|ts|ts.epoch()).unwrap_or_default());
            }) => {
                break result
            }
        }
    }
}

pub(super) async fn start_services(
    start_time: chrono::DateTime<chrono::Utc>,
    opts: &CliOpts,
    mut config: Config,
    shutdown_send: mpsc::Sender<()>,
    rpc_stop_handle: jsonrpsee::server::StopHandle,
    on_app_context_and_db_initialized: impl FnOnce(&AppContext, SyncStatus),
) -> anyhow::Result<()> {
    // Cleanup the collector prometheus metrics registry on start
    crate::metrics::reset_collector_registry();
    let mut services = JoinSet::new();
    let network = config.chain();
    let ctx = AppContext::init(opts, &config).await?;
    info!("Using network :: {network}");
    utils::misc::display_chain_logo(config.chain());
    if opts.exit_after_init {
        return Ok(());
    }
    if !opts.stateless
        && !opts.skip_load_actors
        && let Err(e) = ctx.state_manager.maybe_rewind_heaviest_tipset()
    {
        tracing::warn!("error in maybe_rewind_heaviest_tipset: {e}");
    }
    let p2p_service = create_p2p_service(&mut services, &mut config, &ctx).await?;
    let mpool = create_mpool(&mut services, &p2p_service, &ctx)?;
    let chain_follower = create_chain_follower(opts, &p2p_service, mpool.clone(), &ctx)?;

    maybe_start_rpc_service(
        &mut services,
        &config,
        mpool.clone(),
        &chain_follower,
        start_time,
        shutdown_send.clone(),
        rpc_stop_handle,
        &ctx,
    )?;

    maybe_import_snapshot(opts, &mut config, &ctx).await?;
    if opts.halt_after_import {
        // Cancel all async services
        services.shutdown().await;
        return Ok(());
    }
    on_app_context_and_db_initialized(&ctx, chain_follower.sync_status.clone());
    warmup_in_background(&ctx);
    ctx.state_manager.populate_cache();
    maybe_start_metrics_service(&mut services, &config, &ctx).await?;
    maybe_start_f3_service(opts, &config, &ctx)?;
    maybe_start_health_check_service(&mut services, &config, &p2p_service, &chain_follower, &ctx)
        .await?;
    maybe_start_indexer_service(&mut services, opts, &config, &ctx);
    if !opts.stateless {
        ensure_proof_params_downloaded().await?;
    }
    services.spawn(p2p_service.run());
    start_chain_follower_service(&mut services, chain_follower);
    // blocking until any of the services returns an error,
    propagate_error(&mut services)
        .await
        .context("services failure")
        .map(|_| {})
}

fn warmup_in_background(ctx: &AppContext) {
    // Populate `tipset_by_height` cache
    let cs = ctx.chain_store().clone();
    tokio::task::spawn_blocking(move || {
        let start = Instant::now();
        match cs.chain_index().tipset_by_height(
            // 0 would short-circuit the cache
            1,
            cs.heaviest_tipset(),
            ResolveNullTipset::TakeOlder,
        ) {
            Ok(_) => {
                tracing::info!(
                    "Successfully populated tipset_by_height cache, took {}",
                    humantime::format_duration(start.elapsed())
                );
            }
            Err(e) => {
                tracing::warn!("Failed to populate tipset_by_height cache: {e}");
            }
        }
    });
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
            println!(
                "Forest requires a snapshot to sync with the network, but automatic fetching is disabled."
            );
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
                bail!(
                    "Forest requires a snapshot to sync with the network, but automatic fetching is disabled."
                )
            }
            config.client.snapshot_path = Some(url.to_string().into());
        }
    };

    Ok(())
}

/// returns the first error with which any of the services end, or never returns at all
// This should return anyhow::Result<!> once the `Never` type is stabilized
async fn propagate_error(
    services: &mut JoinSet<Result<(), anyhow::Error>>,
) -> anyhow::Result<std::convert::Infallible> {
    while let Some(result) = services.join_next().await {
        if let Ok(Err(error_message)) = result {
            return Err(error_message);
        }
    }
    std::future::pending().await
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
