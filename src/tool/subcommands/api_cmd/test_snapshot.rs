// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    chain::ChainStore,
    chain_sync::{network_context::SyncNetworkContext, SyncConfig, SyncStage},
    db::MemoryDB,
    genesis::{get_network_name_from_genesis, read_genesis_header},
    libp2p::{NetworkMessage, PeerManager},
    lotus_json::HasLotusJson,
    message_pool::{MessagePool, MpoolRpcProvider},
    networks::ChainConfig,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcTestSnapshot {
    pub name: String,
    pub params: serde_json::Value,
    pub response: Result<serde_json::Value, String>,
    #[serde(with = "crate::lotus_json::base64_standard")]
    pub db: Vec<u8>,
}

pub async fn run_test_from_snapshot(path: &Path) -> anyhow::Result<()> {
    CurrentNetwork::set_global(Network::Testnet);
    let mut run = false;
    let snapshot_bytes = std::fs::read(path)?;
    let snapshot_bytes = if let Ok(bytes) = zstd::decode_all(snapshot_bytes.as_slice()) {
        bytes
    } else {
        snapshot_bytes
    };
    let snapshot: RpcTestSnapshot = serde_json::from_slice(snapshot_bytes.as_slice())?;
    let db = Arc::new(MemoryDB::deserialize_from(snapshot.db.as_slice())?);
    let chain_config = Arc::new(ChainConfig::calibnet());
    let (ctx, _, _) = ctx(db, chain_config).await?;
    let params_raw = match serde_json::to_string(&snapshot.params)? {
        s if s.is_empty() => None,
        s => Some(s),
    };

    macro_rules! run_test {
        ($ty:ty) => {
            if snapshot.name.as_str() == <$ty>::NAME {
                let params = <$ty>::parse_params(params_raw.clone(), ParamStructure::Either)?;
                let result = <$ty>::handle(ctx.clone(), params)
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.into_lotus_json_value().map_err(|e| e.to_string()));
                assert_eq!(snapshot.response, result);
                run = true;
            }
        };
    }

    crate::for_each_method!(run_test);

    assert!(run, "RPC method not found");

    Ok(())
}

async fn ctx(
    db: Arc<MemoryDB>,
    chain_config: Arc<ChainConfig>,
) -> anyhow::Result<(
    Arc<RPCState<MemoryDB>>,
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
            db,
            chain_config.clone(),
            genesis_header.clone(),
        )
        .unwrap(),
    );

    let state_manager =
        Arc::new(StateManager::new(chain_store.clone(), chain_config, sync_config).unwrap());
    let network_name = get_network_name_from_genesis(&genesis_header, &state_manager)?;
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
    use crate::daemon::db_util::download_to;
    use itertools::Itertools as _;
    use url::Url;

    #[tokio::test]
    async fn rpc_regression_tests() {
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
            })
            .collect_vec();
        for url in urls {
            print!("Testing {url} ...");
            let tmp_dir = tempfile::tempdir().unwrap();
            let tmp = tempfile::NamedTempFile::new_in(&tmp_dir)
                .unwrap()
                .into_temp_path();
            println!("start downloading at {}", tmp.display());
            download_to(&url, &tmp).await.unwrap();
            println!("done downloading {}", tmp.display());
            run_test_from_snapshot(&tmp).await.unwrap();
            println!("  succeeded.");
        }
    }
}
