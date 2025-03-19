// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::auth::generate_priv_key;
use crate::chain::ChainStore;
use crate::chain_sync::network_context::SyncNetworkContext;
use crate::chain_sync::{SyncConfig, SyncStage};
use crate::cli_shared::cli::EventsConfig;
use crate::cli_shared::snapshot::TrustedVendor;
use crate::daemon::db_util::populate_eth_mappings;
use crate::db::{car::ManyCar, MemoryDB};
use crate::genesis::read_genesis_header;
use crate::key_management::{KeyStore, KeyStoreConfig};
use crate::libp2p::PeerManager;
use crate::message_pool::{MessagePool, MpoolRpcProvider};
use crate::networks::{ChainConfig, NetworkChain};
use crate::rpc::eth::filter::EthEventHandler;
use crate::rpc::{start_rpc, RPCState};
use crate::shim::address::{CurrentNetwork, Network};
use crate::state_manager::StateManager;
use crate::utils::net::{download_to, DownloadFileOption};
use crate::JWT_IDENTIFIER;
use anyhow::Context as _;
use fvm_ipld_blockstore::Blockstore;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
};
use tokio::{
    signal::{
        ctrl_c,
        unix::{signal, SignalKind},
    },
    sync::{mpsc, RwLock},
    task::JoinSet,
};
use tracing::{info, warn};

pub async fn start_offline_server(
    snapshot_files: Vec<PathBuf>,
    chain: NetworkChain,
    rpc_port: u16,
    auto_download_snapshot: bool,
    height: i64,
    genesis: Option<PathBuf>,
    save_jwt_token: Option<PathBuf>,
) -> anyhow::Result<()> {
    info!("Configuring Offline RPC Server");

    let db = Arc::new(ManyCar::new(MemoryDB::default()));

    let snapshot_files = handle_snapshots(
        snapshot_files,
        &chain,
        auto_download_snapshot,
        genesis.clone(),
    )
    .await?;

    db.read_only_files(snapshot_files.iter().cloned())?;
    let chain_config = Arc::new(handle_chain_config(&chain)?);
    let sync_config = Arc::new(SyncConfig::default());
    let events_config = Arc::new(EventsConfig::default());
    let genesis_header = read_genesis_header(
        genesis.as_deref(),
        chain_config.genesis_bytes(&db).await?.as_deref(),
        &db,
    )
    .await?;
    let chain_store = Arc::new(ChainStore::new(
        db.clone(),
        db.clone(),
        db.clone(),
        chain_config.clone(),
        genesis_header.clone(),
    )?);
    let state_manager = Arc::new(StateManager::new(
        chain_store.clone(),
        chain_config,
        sync_config,
    )?);
    let head_ts = Arc::new(db.heaviest_tipset()?);

    populate_eth_mappings(&state_manager, &head_ts)?;

    let (network_send, _) = flume::bounded(5);
    let (tipset_send, _) = flume::bounded(5);
    let network_name = state_manager.get_network_name_from_genesis()?;
    let message_pool: MessagePool<MpoolRpcProvider<ManyCar>> = MessagePool::new(
        MpoolRpcProvider::new(chain_store.publisher().clone(), state_manager.clone()),
        network_name.clone(),
        network_send.clone(),
        Default::default(),
        state_manager.chain_config().clone(),
        &mut JoinSet::new(),
    )?;

    // Validate tipsets since the {height} EPOCH when `height >= 0`,
    // or valiadte the last {-height} EPOCH(s) when `height < 0`
    let n_ts_to_validate = if height > 0 {
        (head_ts.epoch() - height).max(0)
    } else {
        -height
    } as usize;
    if n_ts_to_validate > 0 {
        state_manager.validate_tipsets(head_ts.chain_arc(&db).take(n_ts_to_validate))?;
    }

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
    info!("Admin token: {token}");
    if let Some(path) = save_jwt_token {
        std::fs::write(path, token)?;
    }

    let peer_manager = Arc::new(PeerManager::default());
    let sync_network_context =
        SyncNetworkContext::new(network_send, peer_manager, state_manager.blockstore_owned());

    let rpc_state = RPCState {
        state_manager,
        keystore: Arc::new(RwLock::new(keystore)),
        mpool: Arc::new(message_pool),
        bad_blocks: Default::default(),
        msgs_in_tipset: Default::default(),
        sync_state: Arc::new(parking_lot::RwLock::new(Default::default())),
        eth_event_handler: Arc::new(EthEventHandler::from_config(&events_config)),
        sync_network_context,
        network_name,
        start_time: chrono::Utc::now(),
        shutdown,
        tipset_send,
        snapshot_progress_tracker: Arc::new(parking_lot::RwLock::new(Default::default())),
    };
    rpc_state.sync_state.write().set_stage(SyncStage::Idle);
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
    let mut terminate = signal(SignalKind::terminate())?;

    let result = tokio::select! {
        ret = start_rpc(state, rpc_address, None) => ret,
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

async fn handle_snapshots(
    snapshot_files: Vec<PathBuf>,
    chain: &NetworkChain,
    auto_download_snapshot: bool,
    genesis: Option<PathBuf>,
) -> anyhow::Result<Vec<PathBuf>> {
    if !snapshot_files.is_empty() {
        return Ok(snapshot_files);
    }

    if snapshot_files.is_empty() && chain.is_devnet() {
        return Ok(vec![genesis.context("missing genesis file")?]);
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

fn handle_chain_config(chain: &NetworkChain) -> anyhow::Result<ChainConfig> {
    info!("Using chain config for {chain}");
    let chain_config = ChainConfig::from_chain(chain);
    if chain_config.is_testnet() {
        CurrentNetwork::set_global(Network::Testnet);
    }
    Ok(chain_config)
}
