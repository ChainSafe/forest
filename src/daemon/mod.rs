// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod bundle;
mod context;
pub mod db_util;
pub mod main;

use crate::blocks::Tipset;
use crate::chain::HeadChange;
use crate::chain_sync::ChainFollower;
use crate::chain_sync::network_context::SyncNetworkContext;
use crate::cli_shared::{car_db_path, snapshot};
use crate::cli_shared::{
    chain_path,
    cli::{CliOpts, Config},
};
use crate::daemon::context::{AppContext, DbType};
use crate::daemon::db_util::{
    import_chain_as_forest_car, load_all_forest_cars, populate_eth_mappings,
};
use crate::db::SettingsStore;
use crate::db::car::ManyCar;
use crate::db::{MarkAndSweep, MemoryDB, SettingsExt, ttl::EthMappingCollector};
use crate::libp2p::{Libp2pService, PeerManager};
use crate::message_pool::{MessagePool, MpoolConfig, MpoolRpcProvider};
use crate::networks::{self, ChainConfig};
use crate::rpc::RPCState;
use crate::rpc::eth::filter::EthEventHandler;
use crate::rpc::start_rpc;
use crate::shim::clock::ChainEpoch;
use crate::shim::version::NetworkVersion;
use crate::state_manager::StateManager;
use crate::utils;
use crate::utils::{
    monitoring::MemStatsTracker, proofs_api::ensure_proof_params_downloaded,
    version::FOREST_VERSION_STRING,
};
use anyhow::{Context as _, bail};
use dialoguer::theme::ColorfulTheme;
use futures::{Future, FutureExt, select};
use fvm_ipld_blockstore::Blockstore;
use once_cell::sync::Lazy;
use raw_sync_2::events::{Event, EventInit as _, EventState};
use shared_memory::ShmemConf;
use std::path::Path;
use std::time::Duration;
use std::{cmp, sync::Arc};
use tempfile::{Builder, TempPath};
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
        unsafe { std::env::set_var("FOREST_F3_SIDECAR_FFI_ENABLED", "0") };
    }
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
    if !opts.skip_load.unwrap_or_default() {
        if let Some(path) = &config.client.snapshot_path {
            let (car_db_path, ts) = import_chain_as_forest_car(
                path,
                &ctx.db_meta_data.get_forest_car_db_dir(),
                config.client.import_mode,
                &snapshot_tracker,
            )
            .await?;
            ctx.db
                .read_only_files(std::iter::once(car_db_path.clone()))?;
            let ts_epoch = ts.epoch();
            // Explicitly set heaviest tipset here in case HEAD_KEY has already been set
            // in the current setting store
            ctx.state_manager
                .chain_store()
                .set_heaviest_tipset(ts.into())?;
            debug!(
                "Loaded car DB at {} and set current head to epoch {ts_epoch}",
                car_db_path.display(),
            );
        }
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
        services.spawn(async {
            crate::metrics::init_prometheus(prometheus_listener, db_directory, db)
                .await
                .context("Failed to initiate prometheus server")
        });

        crate::metrics::default_registry().register_collector(Box::new(
            networks::metrics::NetworkHeightCollector::new(
                ctx.state_manager.chain_config().block_delay_secs,
                ctx.state_manager
                    .chain_store()
                    .genesis_block_header()
                    .timestamp,
            ),
        ));
    }
    Ok(())
}

fn maybe_start_gc_service(
    services: &mut JoinSet<anyhow::Result<()>>,
    opts: &CliOpts,
    config: &Config,
    ctx: &AppContext,
) {
    if !opts.no_gc {
        let mut db_garbage_collector = {
            let chain_store = ctx.state_manager.chain_store().clone();
            let depth = cmp::max(
                ctx.state_manager.chain_config().policy.chain_finality * 2,
                config.sync.recent_state_roots,
            );

            let get_heaviest_tipset = Box::new(move || chain_store.heaviest_tipset());

            MarkAndSweep::new(
                ctx.db.writer().clone(),
                get_heaviest_tipset,
                depth,
                Duration::from_secs(ctx.state_manager.chain_config().block_delay_secs as u64),
            )
        };

        services.spawn(async move { db_garbage_collector.gc_loop(GC_INTERVAL).await });
    }
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
        ctx.network_name.as_str(),
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
        ctx.network_name.clone(),
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
        Arc::new(Tipset::from(
            ctx.state_manager.chain_store().genesis_block_header(),
        )),
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
            sync_states: chain_follower.sync_states.clone(),
            peer_manager: p2p_service.peer_manager().clone(),
            settings_store: ctx.db.writer().clone(),
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
        services.spawn({
            let state_manager = ctx.state_manager.clone();
            let bad_blocks = chain_follower.bad_blocks.clone();
            let sync_states = chain_follower.sync_states.clone();
            let sync_status = chain_follower.sync_status.clone();
            let sync_network_context = chain_follower.network.clone();
            let tipset_send = chain_follower.tipset_sender.clone();
            let keystore = ctx.keystore.clone();
            let network_name = ctx.network_name.clone();
            let snapshot_progress_tracker = ctx.snapshot_progress_tracker.clone();
            let msgs_in_tipset = Arc::new(crate::chain::MsgsInTipsetCache::default());
            async move {
                start_rpc(
                    RPCState {
                        state_manager,
                        keystore,
                        mpool,
                        bad_blocks,
                        msgs_in_tipset,
                        sync_states,
                        sync_status,
                        eth_event_handler,
                        sync_network_context,
                        network_name,
                        start_time,
                        shutdown,
                        tipset_send,
                        snapshot_progress_tracker,
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
    ctx: &AppContext,
) {
    if !config.client.enable_rpc {
        if crate::f3::is_sidecar_ffi_enabled(ctx.state_manager.chain_config()) {
            tracing::warn!("F3 sidecar is enabled but not run because RPC is disabled. ")
        }
        return;
    }

    if !opts.halt_after_import && !opts.stateless {
        let rpc_address = config.client.rpc_address;
        let state_manager = &ctx.state_manager;
        let p2p_peer_id = ctx.p2p_peer_id;
        let admin_jwt = ctx.admin_jwt.clone();
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
            } = crate::f3::get_f3_sidecar_params(&chain_config);
            move || {
                crate::f3::run_f3_sidecar_if_enabled(
                    &chain_config,
                    format!("http://{rpc_address}/rpc/v1"),
                    admin_jwt,
                    crate::rpc::f3::get_f3_rpc_endpoint().to_string(),
                    initial_power_table
                        .map(|i| i.to_string())
                        .unwrap_or_default(),
                    bootstrap_epoch,
                    chain_finality,
                    std::env::var("FOREST_F3_ROOT")
                        .unwrap_or(default_f3_root.display().to_string()),
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
    ctx: &AppContext,
) {
    if config.chain_indexer.enable_indexer
        && !opts.stateless
        && !ctx.state_manager.chain_config().is_devnet()
    {
        let state_manager = ctx.state_manager.clone();
        let settings = ctx.db.writer().clone();
        services.spawn(async move {
            if let Err(err) = init_ethereum_mapping(state_manager, &settings, &config) {
                tracing::warn!("Init Ethereum mapping failed: {}", err)
            }
            Ok(())
        });
    }
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
                    chain_store.db.clone(),
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
    mut config: Config,
    shutdown_send: mpsc::Sender<()>,
) -> anyhow::Result<()> {
    startup_init(&opts, &config)?;
    let mut services = JoinSet::new();
    maybe_start_track_peak_rss_service(&mut services, &opts);
    let ctx = AppContext::init(&opts, &config).await?;
    info!(
        "Using network :: {}",
        get_actual_chain_name(&ctx.network_name)
    );
    utils::misc::display_chain_logo(config.chain());
    if opts.exit_after_init {
        return Ok(());
    }
    let p2p_service = create_p2p_service(&mut services, &mut config, &ctx).await?;

    let mpool = create_mpool(&mut services, &p2p_service, &ctx)?;

    let chain_follower = create_chain_follower(&opts, &p2p_service, mpool.clone(), &ctx)?;

    info!(
        "Starting network:: {}",
        get_actual_chain_name(&ctx.network_name)
    );

    maybe_start_rpc_service(
        &mut services,
        &config,
        mpool.clone(),
        &chain_follower,
        start_time,
        shutdown_send.clone(),
        &ctx,
    )?;

    maybe_import_snapshot(&opts, &mut config, &ctx).await?;
    if opts.halt_after_import {
        // Cancel all async services
        services.shutdown().await;
        return Ok(());
    }
    ctx.state_manager.populate_cache();
    maybe_start_metrics_service(&mut services, &config, &ctx).await?;
    maybe_start_gc_service(&mut services, &opts, &config, &ctx);
    maybe_start_f3_service(&mut services, &opts, &config, &ctx);
    maybe_start_health_check_service(&mut services, &config, &p2p_service, &chain_follower, &ctx)
        .await?;
    maybe_populate_eth_mappings_in_background(&mut services, &opts, config.clone(), &ctx);
    maybe_start_indexer_service(&mut services, &opts, &config, &ctx);
    if !opts.stateless {
        ensure_proof_params_downloaded().await?;
    }
    services.spawn(p2p_service.run());
    start_chain_follower_service(&mut services, chain_follower);
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

fn init_ethereum_mapping<DB: Blockstore>(
    state_manager: Arc<StateManager<DB>>,
    settings: &impl SettingsStore,
    config: &Config,
) -> anyhow::Result<()> {
    match settings.eth_mapping_up_to_date()? {
        Some(false) | None => {
            let car_db_path = car_db_path(config)?;
            let db: Arc<ManyCar<MemoryDB>> = Arc::default();
            load_all_forest_cars(&db, &car_db_path)?;
            let ts = db.heaviest_tipset()?;

            populate_eth_mappings(&state_manager, &ts)?;

            settings.set_eth_mapping_up_to_date()
        }
        Some(true) => {
            tracing::info!("Ethereum mapping up to date");
            Ok(())
        }
    }
}
