// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    chain::ChainStore,
    chain_sync::{network_context::SyncNetworkContext, SyncConfig, SyncStage},
    db::{
        car::{AnyCar, ManyCar},
        MemoryDB,
    },
    genesis::read_genesis_header,
    libp2p::{NetworkMessage, PeerManager},
    lotus_json::HasLotusJson,
    message_pool::{MessagePool, MpoolRpcProvider},
    networks::{ChainConfig, NetworkChain},
    rpc::{eth::filter::EthEventHandler, RPCState, RpcMethod as _, RpcMethodExt as _},
    shim::address::{CurrentNetwork, Network},
    state_manager::StateManager,
    KeyStore, KeyStoreConfig,
};
use openrpc_types::ParamStructure;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};
use tokio::{sync::mpsc, task::JoinSet};

#[derive(Default, Hash, Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct Payload(#[serde(with = "crate::lotus_json::base64_standard")] pub Vec<u8>);

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub eth_mappings: std::collections::HashMap<String, Payload>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcTestSnapshot {
    pub chain: NetworkChain,
    pub name: String,
    pub params: serde_json::Value,
    pub response: Result<serde_json::Value, String>,
    pub index: Index,
    #[serde(with = "crate::lotus_json::base64_standard")]
    pub db: Vec<u8>,
}

pub async fn run_test_from_snapshot(path: &Path) -> anyhow::Result<()> {
    let mut run = false;
    let snapshot_bytes = std::fs::read(path)?;
    let snapshot_bytes = if let Ok(bytes) = zstd::decode_all(snapshot_bytes.as_slice()) {
        bytes
    } else {
        snapshot_bytes
    };
    let RpcTestSnapshot {
        chain,
        name: method_name,
        params,
        index,
        db: db_bytes,
        response: expected_response,
    } = serde_json::from_slice(snapshot_bytes.as_slice())?;
    if chain.is_testnet() {
        CurrentNetwork::set_global(Network::Testnet);
    }
    let db = Arc::new(ManyCar::new(MemoryDB::default()).with_read_only(AnyCar::new(db_bytes)?)?);
    let chain_config = Arc::new(ChainConfig::from_chain(&chain));
    let (ctx, _, _) = ctx(db, chain_config).await?;
    let params_raw = match serde_json::to_string(&params)? {
        s if s.is_empty() => None,
        s => Some(s),
    };

    // backfill db with index data

    macro_rules! run_test {
        ($ty:ty) => {
            if method_name.as_str() == <$ty>::NAME {
                let params = <$ty>::parse_params(params_raw.clone(), ParamStructure::Either)?;
                let result = <$ty>::handle(ctx.clone(), params)
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.into_lotus_json_value().map_err(|e| e.to_string()));
                assert_eq!(expected_response, result);
                run = true;
            }
        };
    }

    crate::for_each_rpc_method!(run_test);

    assert!(run, "RPC method not found");

    Ok(())
}

async fn ctx(
    db: Arc<ManyCar<MemoryDB>>,
    chain_config: Arc<ChainConfig>,
) -> anyhow::Result<(
    Arc<RPCState<ManyCar<MemoryDB>>>,
    flume::Receiver<NetworkMessage>,
    tokio::sync::mpsc::Receiver<()>,
)> {
    let (network_send, network_rx) = flume::bounded(5);
    let (tipset_send, _) = flume::bounded(5);
    let sync_config = Arc::new(SyncConfig::default());
    let genesis_header =
        read_genesis_header(None, chain_config.genesis_bytes(&db).await?.as_deref(), &db).await?;
    let chain_store = Arc::new(
        ChainStore::new(
            db.clone(),
            db.clone(),
            db.clone(),
            chain_config.clone(),
            genesis_header.clone(),
        )
        .unwrap(),
    );
    chain_store.set_heaviest_tipset(db.heaviest_tipset()?.into())?;
    let state_manager =
        Arc::new(StateManager::new(chain_store.clone(), chain_config, sync_config).unwrap());
    let network_name = state_manager.get_network_name_from_genesis()?;
    let message_pool = MessagePool::new(
        MpoolRpcProvider::new(chain_store.publisher().clone(), state_manager.clone()),
        network_name.clone(),
        network_send.clone(),
        Default::default(),
        state_manager.chain_config().clone(),
        &mut JoinSet::new(),
    )?;

    let peer_manager = Arc::new(PeerManager::default());
    let sync_network_context =
        SyncNetworkContext::new(network_send, peer_manager, state_manager.blockstore_owned());
    let (shutdown, shutdown_recv) = mpsc::channel(1);
    let rpc_state = Arc::new(RPCState {
        state_manager,
        keystore: Arc::new(tokio::sync::RwLock::new(KeyStore::new(
            KeyStoreConfig::Memory,
        )?)),
        mpool: Arc::new(message_pool),
        bad_blocks: Default::default(),
        sync_state: Arc::new(RwLock::new(Default::default())),
        eth_event_handler: Arc::new(EthEventHandler::new()),
        sync_network_context,
        network_name,
        start_time: chrono::Utc::now(),
        shutdown,
        tipset_send,
    });
    rpc_state.sync_state.write().set_stage(SyncStage::Idle);
    Ok((rpc_state, network_rx, shutdown_recv))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::net::{download_file_with_cache, DownloadFileOption};
    use directories::ProjectDirs;
    use futures::{stream::FuturesUnordered, StreamExt};
    use itertools::Itertools as _;
    use tokio::sync::Semaphore;
    use url::Url;

    #[tokio::test(flavor = "multi_thread")]
    async fn rpc_regression_tests() {
        // Skip for debug build on CI as the downloading is slow and flaky
        if crate::utils::is_ci() && crate::utils::is_debug_build() {
            return;
        }

        let urls = include_str!("test_snapshots.txt")
            .trim()
            .split("\n")
            .filter_map(|n| {
                Url::parse(
                    format!(
                        "https://forest-snapshots.fra1.cdn.digitaloceanspaces.com/rpc_test/{n}"
                    )
                    .as_str(),
                )
                .ok()
                .map(|url| (n, url))
            })
            .collect_vec();
        let project_dir = ProjectDirs::from("com", "ChainSafe", "Forest").unwrap();
        let cache_dir = project_dir.cache_dir().join("test").join("rpc-snapshots");
        let semaphore = Arc::new(Semaphore::new(4));
        let mut tasks = FuturesUnordered::from_iter(urls.into_iter().map(|(filename, url)| {
            let cache_dir = cache_dir.clone();
            let semaphore = semaphore.clone();
            async move {
                let _permit = semaphore.acquire().await.unwrap();
                let result =
                    download_file_with_cache(&url, &cache_dir, DownloadFileOption::NonResumable)
                        .await
                        .unwrap();
                (filename, result.path)
            }
        }));

        while let Some((filename, file_path)) = tasks.next().await {
            print!("Testing {filename} ...");
            run_test_from_snapshot(&file_path).await.unwrap();
            println!("  succeeded.");
        }
    }
}
