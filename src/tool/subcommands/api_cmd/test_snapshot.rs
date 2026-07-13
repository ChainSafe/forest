// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::{
    KeyStore, KeyStoreConfig,
    chain::ChainStore,
    chain_sync::{SyncStatusReport, network_context::SyncNetworkContext},
    db::{
        MemoryDB,
        car::{AnyCar, ManyCar},
    },
    genesis::read_genesis_header,
    libp2p::{NetworkMessage, PeerManager},
    lotus_json::HasLotusJson,
    message_pool::{MessagePool, MpoolLocker, NonceTracker},
    networks::{ChainConfig, NetworkChain},
    prelude::*,
    rpc::{
        ApiPaths, RPCState, RpcMethod, RpcMethodExt as _,
        eth::{filter::EthEventHandler, types::EthHash},
    },
    shim::{
        address::{CurrentNetwork, Network},
        clock::ChainEpoch,
    },
    state_manager::StateManager,
};
use ahash::HashMap;
use arc_swap::ArcSwap;
use openrpc_types::ParamStructure;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::{path::Path, str::FromStr};
use tokio::{sync::mpsc, task::JoinSet};

#[derive(Default, Hash, Eq, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct Payload(#[serde(with = "crate::lotus_json::base64_standard")] pub Vec<u8>);

#[derive(Default, PartialEq, Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub eth_mappings: Option<ahash::HashMap<String, Payload>>,
    pub indices: Option<ahash::HashMap<String, Payload>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcTestSnapshot {
    pub chain: NetworkChain,
    pub name: String,
    pub params: serde_json::Value,
    pub response: Result<serde_json::Value, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<Index>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tipset_by_epoch: Option<HashMap<ChainEpoch, nunny::Vec<String>>>,
    #[serde(with = "crate::lotus_json::base64_standard")]
    pub db: Vec<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_path: Option<ApiPaths>,
}

fn backfill_eth_mappings(db: &MemoryDB, index: Option<Index>) -> anyhow::Result<()> {
    if let Some(index) = index
        && let Some(mut guard) = db.eth_mappings_db.try_write()
        && let Some(eth_mappings) = index.eth_mappings
    {
        for (k, v) in eth_mappings.iter() {
            guard.insert(EthHash::from_str(k)?, v.0.clone());
        }
    }
    Ok(())
}

/// JSON fields filtered from both actual and expected responses
/// before strict raw-JSON snapshot comparison, scoped to the methods whose
/// responses actually contain them.
fn fields_to_filter(method: &str) -> &'static [&'static str] {
    match method {
        // Skip time taken and duration as they are non-deterministic.
        "Filecoin.StateCall" | "Filecoin.StateReplay" | "Filecoin.StateCompute" => {
            &["Duration", "tt"]
        }
        // Skip `accessList` as it is known-divergent from Lotus.
        // See <https://github.com/filecoin-project/lotus/issues/12214>.
        "Filecoin.EthGetBlockByHash"
        | "Filecoin.EthGetBlockByNumber"
        | "Filecoin.EthGetTransactionByHash"
        | "Filecoin.EthGetTransactionByHashLimited"
        | "Filecoin.EthGetTransactionByBlockHashAndIndex"
        | "Filecoin.EthGetTransactionByBlockNumberAndIndex" => &["accessList"],
        _ => &[],
    }
}

/// Recursively filters `fields` from a JSON value so the strict raw-JSON
/// snapshot comparison ignores them.
fn filter_out_fields(value: &mut serde_json::Value, fields: &[&str]) {
    if fields.is_empty() {
        return;
    }
    match value {
        serde_json::Value::Object(map) => {
            for field in fields {
                map.remove(*field);
            }
            for val in map.values_mut() {
                filter_out_fields(val, fields);
            }
        }
        serde_json::Value::Array(items) => {
            items.iter_mut().for_each(|v| filter_out_fields(v, fields));
        }
        _ => {}
    }
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
        tipset_by_epoch,
        db: db_bytes,
        response: expected_response,
        api_path,
    } = serde_json::from_slice(snapshot_bytes.as_slice()).context("failed to parse snapshot")?;
    if chain.is_testnet() {
        CurrentNetwork::set_global(Network::Testnet);
    }
    let api_path = api_path.unwrap_or(ApiPaths::V1);
    let db = Arc::new(
        ManyCar::new(MemoryDB::default())
            .with_read_only(AnyCar::new(db_bytes)?)
            .context("failed to create db from snapshot")?,
    );
    // backfill tipset by epoch lookup table
    if let Some(tipset_by_epoch) = tipset_by_epoch
        && !tipset_by_epoch.is_empty()
    {
        *db.writer().ts_lookup_db.write() = tipset_by_epoch
            .into_iter()
            .map(|(k, v)| {
                anyhow::Ok((
                    k,
                    nunny::Vec::new(v.into_iter().map(|s| Cid::from_str(&s)).try_collect()?)
                        .map_err(|_| anyhow::anyhow!("infallible NonEmpty conversion"))?
                        .into(),
                ))
            })
            .try_collect()?;
    }
    // backfill db with index data
    backfill_eth_mappings(db.writer(), index)
        .context("failed to backfill eth mappings from index")?;
    let chain_config = Arc::new(ChainConfig::from_chain(&chain));
    let (ctx, _, _) = ctx(db, chain_config)
        .await
        .context("failed to create RPC context")?;
    let params_raw =
        match serde_json::to_string(&params).context("failed to serialize params to string")? {
            s if s.is_empty() => None,
            s => Some(s),
        };
    let mut ext = http::Extensions::new();
    ext.insert(api_path);
    macro_rules! run_test {
        ($ty:ty) => {
            if (method_name.as_str() == <$ty>::NAME
                || Some(method_name.as_ref()) == <$ty>::NAME_ALIAS)
                && <$ty>::API_PATHS.contains(api_path)
            {
                let params = <$ty>::parse_params(params_raw.clone(), ParamStructure::Either)
                    .context("failed to parse params")?;
                let mut result = <$ty>::handle(ctx.clone(), params, &ext)
                    .await
                    .map_err(|e| e.deref().to_string())
                    .and_then(|r| r.into_lotus_json_value().map_err(|e| e.to_string()));
                let mut expected = expected_response.clone();
                let fields = fields_to_filter(<$ty>::NAME);
                if let Ok(v) = result.as_mut() {
                    filter_out_fields(v, fields);
                }
                if let Ok(v) = expected.as_mut() {
                    filter_out_fields(v, fields);
                }
                pretty_assertions::assert_eq!(result, expected);
                run = true;
            }
        };
    }

    crate::for_each_rpc_method!(run_test);

    assert!(run, "RPC method {method_name} not found");

    Ok(())
}

async fn ctx(
    db: Arc<ManyCar<MemoryDB>>,
    chain_config: Arc<ChainConfig>,
) -> anyhow::Result<(
    Arc<RPCState>,
    flume::Receiver<NetworkMessage>,
    tokio::sync::mpsc::Receiver<()>,
)> {
    let (network_send, network_rx) = flume::bounded(5);
    let (tipset_send, _) = flume::bounded(5);
    let genesis_header =
        read_genesis_header(None, chain_config.genesis_bytes(&db).await?.as_deref(), &db).await?;
    let chain_store = ChainStore::new(db, chain_config, genesis_header.clone())?;
    let state_manager = StateManager::new(chain_store.shallow_clone())
        .unwrap()
        // cache must be disabled to avoid flakiness in RPC regression tests
        .with_id_address_cache_disabled();
    let mut services: JoinSet<anyhow::Result<()>> = JoinSet::new();
    let message_pool = MessagePool::new(
        chain_store,
        network_send.clone(),
        Default::default(),
        state_manager.chain_config().clone(),
        &mut services,
    )?;
    // The mpool services are not needed in this snapshot test context; abort
    // them right away so they don't compete with the test for runtime time
    // (the inherited `&mut JoinSet::new()` pattern was a temporary that
    // dropped — same end state). The detached drain still polls the aborted
    // set so any pre-abort error or panic is surfaced rather than dropped.
    services.abort_all();
    tokio::spawn(drain_mpool_services(services));

    let peer_manager = Arc::new(PeerManager::default());
    let sync_network_context =
        SyncNetworkContext::new(network_send, peer_manager, state_manager.db_owned());
    let (shutdown, shutdown_recv) = mpsc::channel(1);
    let nonce_tracker = NonceTracker::new();
    let eth_event_handler = Arc::new(EthEventHandler::from_config(
        &crate::cli_shared::cli::EventsConfig::default(),
        state_manager.chain_config().eth_chain_id,
        message_pool.subscriber(),
    ));
    let rpc_state = Arc::new(RPCState {
        state_manager,
        keystore: Arc::new(RwLock::new(KeyStore::new(KeyStoreConfig::Memory)?)),
        mpool: message_pool,
        bad_blocks: Default::default(),
        sync_status: Arc::new(ArcSwap::from_pointee(SyncStatusReport::init())),
        eth_event_handler,
        eth_logs_feed: Default::default(),
        sync_network_context,
        start_time: chrono::Utc::now(),
        shutdown,
        tipset_send,
        snapshot_progress_tracker: Default::default(),
        mpool_locker: MpoolLocker::new(),
        nonce_tracker,
        temp_dir: Arc::new(std::env::temp_dir()),
    });
    Ok((rpc_state, network_rx, shutdown_recv))
}

/// Drains a `MessagePool` service [`JoinSet`] to completion, logging any
/// errors or panics it produces. Intended to be used with `tokio::spawn` from
/// test-utility `ctx()` helpers so that service-task errors are surfaced
/// instead of being silently dropped when the `JoinSet` is dropped.
pub(super) async fn drain_mpool_services(mut services: JoinSet<anyhow::Result<()>>) {
    while let Some(result) = services.join_next().await {
        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!("message pool service task error: {e:#}"),
            Err(je) if je.is_cancelled() => {}
            Err(je) => tracing::warn!("message pool service task panicked: {je}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::proofs_api::ensure_proof_params_downloaded;
    use ahash::HashSet;
    use std::sync::LazyLock;
    use std::time::{Duration, Instant};

    // To run a single test: cargo test --lib filecoin_multisig_statedecodeparams_1754230255631789 -- --nocapture
    include!(concat!(env!("OUT_DIR"), "/__rpc_regression_tests_gen.rs"));

    // Per-test timeout so a hang surfaces as a `fickle`-retryable panic
    // instead of stalling the whole CI job.
    const RPC_REGRESSION_TEST_TIMEOUT: Duration = Duration::from_secs(300);

    // `std::env::set_var` is not thread-safe on Linux, so
    // initialize once per process — never concurrently from parallel tests.
    static INIT_RNG_SEED: LazyLock<()> = LazyLock::new(|| {
        if std::env::var(crate::utils::rand::FIXED_RNG_SEED_ENV).is_err() {
            unsafe { std::env::set_var(crate::utils::rand::FIXED_RNG_SEED_ENV, "4213666") };
        }
    });

    #[allow(dead_code)]
    async fn rpc_regression_test_run(name: &str) {
        LazyLock::force(&INIT_RNG_SEED);
        tokio::time::timeout(RPC_REGRESSION_TEST_TIMEOUT, async {
            crate::utils::proofs_api::maybe_set_proofs_parameter_cache_dir_env(
                &crate::cli_shared::default_data_dir(),
            );
            ensure_proof_params_downloaded().await.unwrap();
            let path = crate::dev::subcommands::fetch_rpc_test_snapshot(name.into())
                .await
                .unwrap();

            print!("Testing {name} ...");
            let start = Instant::now();
            run_test_from_snapshot(&path).await.unwrap();
            println!(
                "  succeeded, took {}.",
                humantime::format_duration(start.elapsed())
            );
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "rpc regression test {name} timed out after {}",
                humantime::format_duration(RPC_REGRESSION_TEST_TIMEOUT)
            )
        });
    }

    #[test]
    fn rpc_regression_tests_print_uncovered() {
        let pattern =
            lazy_regex::regex!(r#"^(?P<name>(filecoin|eth)_.+)_\d+\.rpcsnap\.json\.zst$"#);
        let covered = HashSet::from_iter(
            include_str!("test_snapshots.txt")
                .trim()
                .split("\n")
                .map(|i| {
                    // Remove comment
                    let i = i.split("#").next().unwrap().trim();
                    let captures = pattern.captures(i).expect("pattern capture failure");
                    captures
                        .name("name")
                        .expect("no named capture group")
                        .as_str()
                        .replace("_", ".")
                        .to_lowercase()
                        .replace("eth.", "eth_")
                }),
        );
        println!("covered: {covered:?}");
        let ignored = HashSet::from_iter(
            include_str!("test_snapshots_ignored.txt")
                .trim()
                .split("\n")
                .map(str::to_lowercase),
        );
        println!("ignored: {ignored:?}");

        let mut uncovered = vec![];

        macro_rules! print_uncovered {
            ($ty:ty) => {
                let name = <$ty>::NAME.to_lowercase();
                if !covered.contains(&name) && !ignored.contains(&name) {
                    let is_covered = if let Some(alias) = <$ty>::NAME_ALIAS {
                        let alias = alias.to_lowercase();
                        covered.contains(&alias) || ignored.contains(&alias)
                    } else {
                        false
                    };
                    if !is_covered {
                        uncovered.push(<$ty>::NAME);
                    }
                }
            };
        }

        crate::for_each_rpc_method!(print_uncovered);

        if !uncovered.is_empty() {
            uncovered.sort();
            println!("Uncovered RPC methods:");
            for i in uncovered.iter() {
                println!("{i}");
            }
        }

        assert!(
            uncovered.is_empty(),
            "either ignore or upload test snapshots for uncovered RPC methods"
        );
    }
}
