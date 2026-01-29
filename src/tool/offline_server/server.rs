// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::auth::generate_priv_key;
use crate::chain::ChainStore;
use crate::chain_sync::SyncStatusReport;
use crate::chain_sync::network_context::SyncNetworkContext;
use crate::cli_shared::cli::EventsConfig;
use crate::cli_shared::snapshot::TrustedVendor;
use crate::daemon::db_util::RangeSpec;
use crate::daemon::db_util::backfill_db;
use crate::db::{
    EthMappingsStore, HeaviestTipsetKeyProvider, MemoryDB, SettingsStore, car::ManyCar,
};
use crate::genesis::read_genesis_header;
use crate::key_management::{KeyStore, KeyStoreConfig};
use crate::libp2p::PeerManager;
use crate::message_pool::{MessagePool, MpoolRpcProvider};
use crate::networks::{ChainConfig, NetworkChain};
use crate::rpc::eth::filter::EthEventHandler;
use crate::rpc::{RPCState, start_rpc};
use crate::shim::address::{CurrentNetwork, Network};
use crate::state_manager::StateManager;
use crate::utils::net::{DownloadFileOption, download_to};
use crate::utils::proofs_api::{self, ensure_proof_params_downloaded};
use crate::{Config, JWT_IDENTIFIER};
use anyhow::Context as _;
use fvm_ipld_blockstore::Blockstore;
use jsonrpsee::server::stop_channel;
use parking_lot::RwLock;
use std::{
    mem::discriminant,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::{
    signal::{
        ctrl_c,
        unix::{SignalKind, signal},
    },
    sync::mpsc,
    task::JoinSet,
};
use tracing::{info, warn};

/// Builds offline RPC state and returns it with a shutdown receiver.
/// The receiver is notified when RPC shutdown is requested.
pub async fn offline_rpc_state<DB>(
    chain: NetworkChain,
    db: Arc<DB>,
    genesis_fp: Option<&Path>,
    save_jwt_token: Option<&Path>,
    services: &mut JoinSet<anyhow::Result<()>>,
) -> anyhow::Result<(RPCState<DB>, mpsc::Receiver<()>)>
where
    DB: Blockstore
        + SettingsStore
        + HeaviestTipsetKeyProvider
        + EthMappingsStore
        + Send
        + Sync
        + 'static,
{
    let chain_config = Arc::new(handle_chain_config(&chain)?);
    let events_config = Arc::new(EventsConfig::default());
    let genesis_header = read_genesis_header(
        genesis_fp,
        chain_config.genesis_bytes(&db).await?.as_deref(),
        &db,
    )
    .await?;
    // let head_ts = db.heaviest_tipset()?;
    let chain_store = Arc::new(ChainStore::new(
        db.clone(),
        db.clone(),
        db.clone(),
        chain_config,
        genesis_header.clone(),
    )?);
    let state_manager = Arc::new(StateManager::new(chain_store.clone())?);
    let (network_send, _) = flume::bounded(5);
    let (tipset_send, _) = flume::bounded(5);

    let message_pool = MessagePool::new(
        MpoolRpcProvider::new(chain_store.publisher().clone(), state_manager.clone()),
        network_send.clone(),
        Default::default(),
        state_manager.chain_config().clone(),
        services,
    )?;

    let (shutdown, shutdown_recv) = mpsc::channel(1);

    let mut keystore = KeyStore::new(KeyStoreConfig::Memory)?;
    keystore.put(JWT_IDENTIFIER, generate_priv_key())?;
    let ki = keystore.get(JWT_IDENTIFIER)?;
    // Lotus admin tokens do not expire but Forest requires all JWT tokens to
    // have an expiration date. So we set the expiration date to 100 years in
    // the future to match user-visible behavior of Lotus.
    let token_exp = chrono::Duration::days(365 * 100);
    let token = crate::auth::create_token(
        crate::auth::ADMIN.iter().map(ToString::to_string).collect(),
        ki.private_key(),
        token_exp,
    )?;
    if let Some(path) = save_jwt_token {
        crate::utils::io::write_new_sensitive_file(token.as_bytes(), path)?;
        info!("Admin token is saved to {}", path.display());
    } else {
        info!("Admin token generated. Use --save-token to persist it.");
    }

    let peer_manager = Arc::new(PeerManager::default());
    let sync_network_context =
        SyncNetworkContext::new(network_send, peer_manager, state_manager.blockstore_owned());

    Ok((
        RPCState {
            state_manager,
            keystore: Arc::new(RwLock::new(keystore)),
            mpool: Arc::new(message_pool),
            bad_blocks: Default::default(),
            msgs_in_tipset: Default::default(),
            sync_status: Arc::new(RwLock::new(SyncStatusReport::init())),
            eth_event_handler: Arc::new(EthEventHandler::from_config(&events_config)),
            sync_network_context,
            start_time: chrono::Utc::now(),
            shutdown,
            tipset_send,
            snapshot_progress_tracker: Default::default(),
        },
        shutdown_recv,
    ))
}

#[allow(clippy::too_many_arguments)]
pub async fn start_offline_server(
    snapshot_files: Vec<PathBuf>,
    chain: Option<NetworkChain>,
    rpc_port: u16,
    auto_download_snapshot: bool,
    height: i64,
    index_backfill_epochs: usize,
    genesis: Option<PathBuf>,
    save_jwt_token: Option<PathBuf>,
) -> anyhow::Result<()> {
    info!("Configuring Offline RPC Server");

    // Set proof parameter data dir and make sure the proofs are available. Otherwise,
    // validation might fail due to missing proof parameters.
    proofs_api::maybe_set_proofs_parameter_cache_dir_env(&Config::default().client.data_dir);
    ensure_proof_params_downloaded().await?;

    let db = {
        let db = Arc::new(ManyCar::new(MemoryDB::default()));
        let snapshot_files = handle_snapshots(
            snapshot_files,
            chain.as_ref(),
            auto_download_snapshot,
            genesis.clone(),
        )
        .await?;
        db.read_only_files(snapshot_files.iter().cloned())?;
        db
    };

    let inferred_chain = {
        let head = db.heaviest_tipset()?;
        let genesis = head.genesis(&db)?;
        NetworkChain::from_genesis_or_devnet_placeholder(genesis.cid())
    };
    let chain = if let Some(chain) = chain {
        anyhow::ensure!(
            discriminant(&inferred_chain) == discriminant(&chain),
            "chain mismatch, specified: {chain}, actual: {inferred_chain}",
        );
        chain
    } else {
        inferred_chain
    };
    let mut services = JoinSet::new();
    let (rpc_state, shutdown_recv) = offline_rpc_state(
        chain,
        db,
        genesis.as_deref(),
        save_jwt_token.as_deref(),
        &mut services,
    )
    .await?;

    let state_manager = &rpc_state.state_manager;
    let head_ts = state_manager.heaviest_tipset();
    if index_backfill_epochs > 0 {
        backfill_db(
            state_manager,
            &head_ts,
            RangeSpec::NumTipsets(index_backfill_epochs),
        )
        .await?;
    }

    // Validate tipsets since the {height} EPOCH when `height >= 0`,
    // or valiadte the last {-height} EPOCH(s) when `height < 0`
    let validate_until_epoch = if height > 0 {
        height
    } else {
        head_ts.epoch() + height + 1
    };
    if validate_until_epoch <= head_ts.epoch() {
        state_manager.validate_tipsets(
            head_ts
                .chain(rpc_state.store())
                .take_while(|ts| ts.epoch() >= validate_until_epoch),
        )?;
    }

    start_offline_rpc(rpc_state, rpc_port, shutdown_recv).await?;

    Ok(())
}

async fn start_offline_rpc<DB>(
    state: RPCState<DB>,
    rpc_port: u16,
    mut shutdown_recv: mpsc::Receiver<()>,
) -> anyhow::Result<()>
where
    DB: Blockstore + Send + Sync + 'static,
{
    info!("Starting offline RPC Server");
    let rpc_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), rpc_port);
    let rpc_listener = tokio::net::TcpListener::bind(rpc_address).await?;
    let mut terminate = signal(SignalKind::terminate())?;
    let (stop_handle, server_handle) = stop_channel();
    let result = tokio::select! {
        ret = start_rpc(state, rpc_listener,stop_handle, None) => ret,
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
    if let Err(e) = server_handle.stop() {
        tracing::warn!("{e}");
    }
    crate::utils::io::terminal_cleanup();
    result
}

async fn handle_snapshots(
    snapshot_files: Vec<PathBuf>,
    chain: Option<&NetworkChain>,
    auto_download_snapshot: bool,
    genesis: Option<PathBuf>,
) -> anyhow::Result<Vec<PathBuf>> {
    if !snapshot_files.is_empty() {
        return Ok(snapshot_files);
    }
    let chain = chain.context("`--chain` is required when no snapshots are supplied")?;
    if chain.is_devnet() {
        anyhow::ensure!(
            !auto_download_snapshot,
            "auto_download_snapshot is not supported for devnet"
        );
        return Ok(vec![
            genesis.context("genesis must be provided for devnet")?,
        ]);
    }

    let (snapshot_url, num_bytes, path) =
        crate::cli_shared::snapshot::peek(TrustedVendor::default(), chain)
            .await
            .context("couldn't get snapshot size")?;
    if !auto_download_snapshot {
        warn!("Automatic snapshot download is disabled.");
        let message = format!(
            "Fetch a {} snapshot to the current directory? (denying will exit the program). ",
            indicatif::HumanBytes(num_bytes)
        );
        let have_permission =
            dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
                .with_prompt(message)
                .default(false)
                .interact()
                .unwrap_or(false);
        if !have_permission {
            anyhow::bail!("No snapshot provided, exiting offline RPC setup.");
        }
    }
    info!(
        "Downloading latest snapshot for {} size {}",
        chain,
        indicatif::HumanBytes(num_bytes)
    );
    let downloaded_snapshot_path = std::env::current_dir()?.join(path);
    download_to(
        &snapshot_url,
        &downloaded_snapshot_path,
        DownloadFileOption::Resumable,
        None,
    )
    .await?;
    info!("Snapshot downloaded");
    Ok(vec![downloaded_snapshot_path])
}

pub fn handle_chain_config(chain: &NetworkChain) -> anyhow::Result<ChainConfig> {
    info!("Using chain config for {chain}");
    let chain_config = ChainConfig::from_chain(chain);
    if chain_config.is_testnet() {
        CurrentNetwork::set_global(Network::Testnet);
    }
    Ok(chain_config)
}
