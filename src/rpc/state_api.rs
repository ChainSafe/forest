// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use crate::blocks::tipset_keys_json::TipsetKeysJson;
use crate::ipld::json::IpldJson;
use crate::ipld::CidHashSet;
use crate::json::cid::CidJson;
use crate::libp2p::NetworkMessage;
use crate::rpc_api::{
    data_types::{MarketDeal, MessageLookup, RPCState},
    state_api::*,
};
use crate::shim::address::Address;
use crate::state_manager::InvocResult;
use ahash::{HashMap, HashMapExt};
use anyhow::Context;
use cid::Cid;
use fil_actor_interface::market;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_car::CarHeader;
use fvm_ipld_encoding::{CborStore, DAG_CBOR};
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use libipld_core::ipld::Ipld;
use parking_lot::Mutex;
use std::{sync::Arc, time::Duration};
use tokio::task::JoinSet;
use tokio_util::compat::TokioAsyncReadCompatExt;

// TODO handle using configurable verification implementation in RPC (all
// defaulting to Full).

/// runs the given message and returns its result without any persisted changes.
pub(in crate::rpc) async fn state_call<DB: Blockstore + Send + Sync + 'static>(
    data: Data<RPCState<DB>>,
    Params(params): Params<StateCallParams>,
) -> Result<StateCallResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (message_json, key) = params;
    let mut message = message_json.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())?;
    Ok(state_manager.call(&mut message, Some(tipset))?)
}

/// returns the result of executing the indicated message, assuming it was
/// executed in the indicated tipset.
pub(in crate::rpc) async fn state_replay<DB: Blockstore + Send + Sync + 'static>(
    data: Data<RPCState<DB>>,
    Params(params): Params<StateReplayParams>,
) -> Result<StateReplayResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (cidjson, key) = params;
    let cid = cidjson.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())?;
    let (msg, ret) = state_manager.replay(&tipset, cid).await?;

    Ok(InvocResult {
        msg,
        msg_rct: Some(ret.msg_receipt()),
        error: ret.failure_info(),
    })
}

/// gets network name from state manager
pub(in crate::rpc) async fn state_network_name<DB: Blockstore>(
    data: Data<RPCState<DB>>,
) -> Result<StateNetworkNameResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let heaviest_tipset = state_manager.chain_store().heaviest_tipset();

    state_manager
        .get_network_name(heaviest_tipset.parent_state())
        .map_err(|e| e.into())
}

pub(in crate::rpc) async fn state_get_network_version<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(params): Params<StateNetworkVersionParams>,
) -> Result<StateNetworkVersionResult, JsonRpcError> {
    let (TipsetKeysJson(tsk),) = params;
    let ts = data.chain_store.tipset_from_keys(&tsk)?;
    Ok(data.state_manager.get_network_version(ts.epoch()))
}

/// looks up the Escrow and Locked balances of the given address in the Storage
/// Market
pub(in crate::rpc) async fn state_market_balance<DB: Blockstore + Send + Sync + 'static>(
    data: Data<RPCState<DB>>,
    Params(params): Params<StateMarketBalanceParams>,
) -> Result<StateMarketBalanceResult, JsonRpcError> {
    let (address, key) = params;
    let address = address.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())?;
    data.state_manager
        .market_balance(&address, &tipset)
        .map_err(|e| e.into())
}

pub(in crate::rpc) async fn state_market_deals<DB: Blockstore>(
    data: Data<RPCState<DB>>,
    Params(params): Params<StateMarketDealsParams>,
) -> Result<StateMarketDealsResult, JsonRpcError> {
    let (TipsetKeysJson(tsk),) = params;
    let ts = data.chain_store.tipset_from_keys(&tsk)?;
    let actor = data
        .state_manager
        .get_actor(&Address::MARKET_ACTOR, *ts.parent_state())?
        .ok_or("Market actor address could not be resolved")?;
    let market_state =
        market::State::load(data.state_manager.blockstore(), actor.code, actor.state)?;

    let da = market_state.proposals(data.state_manager.blockstore())?;
    let sa = market_state.states(data.state_manager.blockstore())?;

    let mut out = HashMap::new();
    da.for_each(|deal_id, d| {
        let s = sa.get(deal_id)?.unwrap_or(market::DealState {
            sector_start_epoch: -1,
            last_updated_epoch: -1,
            slash_epoch: -1,
        });
        out.insert(
            deal_id.to_string(),
            MarketDeal {
                proposal: d,
                state: s,
            },
        );
        Ok(())
    })?;
    Ok(out)
}

/// returns the message receipt for the given message
pub(in crate::rpc) async fn state_get_receipt<DB: Blockstore + Send + Sync + 'static>(
    data: Data<RPCState<DB>>,
    Params(params): Params<StateGetReceiptParams>,
) -> Result<StateGetReceiptResult, JsonRpcError> {
    let (cidjson, key) = params;
    let state_manager = &data.state_manager;
    let cid = cidjson.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())?;
    state_manager
        .get_receipt(tipset, cid)
        .map(|s| s.into())
        .map_err(|e| e.into())
}
/// looks back in the chain for a message. If not found, it blocks until the
/// message arrives on chain, and gets to the indicated confidence depth.
pub(in crate::rpc) async fn state_wait_msg<DB: Blockstore + Send + Sync + 'static>(
    data: Data<RPCState<DB>>,
    Params(params): Params<StateWaitMsgParams>,
) -> Result<StateWaitMsgResult, JsonRpcError> {
    let (cidjson, confidence) = params;
    let state_manager = &data.state_manager;
    let cid: Cid = cidjson.into();
    let (tipset, receipt) = state_manager.wait_for_message(cid, confidence).await?;
    let tipset = tipset.ok_or("wait for msg returned empty tuple")?;
    let receipt = receipt.ok_or("wait for msg returned empty receipt")?;
    let ipld: Ipld = if receipt.return_data().bytes().is_empty() {
        Ipld::Null
    } else {
        receipt.return_data().deserialize()?
    };
    Ok(MessageLookup {
        receipt: receipt.into(),
        tipset: tipset.key().clone().into(),
        height: tipset.epoch(),
        message: CidJson(cid),
        return_dec: IpldJson(ipld),
    })
}

// Sample CIDs (useful for testing):
//   Mainnet:
//     1,594,681 bafy2bzaceaclaz3jvmbjg3piazaq5dcesoyv26cdpoozlkzdiwnsvdvm2qoqm OhSnap upgrade
//     1_960_320 bafy2bzacec43okhmihmnwmgqspyrkuivqtxv75rpymsdbulq6lgsdq2vkwkcg Skyr upgrade
//     2,833,266 bafy2bzacecaydufxqo5vtouuysmg3tqik6onyuezm6lyviycriohgfnzfslm2
//     2,933,266 bafy2bzacebyp6cmbshtzzuogzk7icf24pt6s5veyq5zkkqbn3sbbvswtptuuu
//   Calibnet:
//     242,150 bafy2bzaceb522vvt3wo7xhleo2dvb7wb7pyydmzlahc4aqd7lmvg3afreejiw
//     630,932 bafy2bzacedidwdsd7ds73t3z76hcjfsaisoxrangkxsqlzih67ulqgtxnypqk
//
/// Traverse an IPLD directed acyclic graph and use libp2p-bitswap to request any missing nodes.
/// This function has two primary uses: (1) Downloading specific state-roots when Forest deviates
/// from the mainline blockchain, (2) fetching historical state-trees to verify past versions of the
/// consensus rules.
pub(in crate::rpc) async fn state_fetch_root<DB: Blockstore + Sync + Send + 'static>(
    data: Data<RPCState<DB>>,
    Params((CidJson(root_cid), save_to_file)): Params<StateFetchRootParams>,
) -> Result<StateFetchRootResult, JsonRpcError> {
    let network_send = data.network_send.clone();
    let db = data.chain_store.db.clone();
    drop(data);

    let (car_tx, car_handle) = if let Some(save_to_file) = save_to_file {
        let (car_tx, car_rx) = flume::bounded(100);
        let header = CarHeader::from(vec![root_cid]);
        let file = tokio::fs::File::create(save_to_file).await?;

        let car_handle = tokio::spawn(async move {
            let mut file = file.compat();
            let mut stream = car_rx.stream();
            header.write_stream_async(&mut file, &mut stream).await
        });

        (Some(car_tx), Some(car_handle))
    } else {
        (None, None)
    };

    const MAX_CONCURRENT_REQUESTS: usize = 64;
    const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

    let mut seen: CidHashSet = CidHashSet::default();
    let mut counter: usize = 0;
    let mut fetched: usize = 0;
    let mut failures: usize = 0;
    let mut task_set = JoinSet::new();

    fn handle_worker(fetched: &mut usize, failures: &mut usize, ret: anyhow::Result<()>) {
        match ret {
            Ok(()) => *fetched += 1,
            Err(msg) => {
                *failures += 1;
                tracing::debug!("Request failed: {msg}");
            }
        }
    }

    // When walking an Ipld graph, we're only interested in the DAG_CBOR encoded nodes.
    let mut get_ipld_link = |ipld: &Ipld| match ipld {
        &Ipld::Link(cid) if cid.codec() == DAG_CBOR && seen.insert(cid) => Some(cid),
        _ => None,
    };

    // Do a depth-first-search of the IPLD graph (DAG). Nodes that are _not_ present in our database
    // are fetched in background tasks. If the number of tasks reaches MAX_CONCURRENT_REQUESTS, the
    // depth-first-search pauses until one of the work tasks returns. The memory usage of this
    // algorithm is dominated by the set of seen CIDs and the 'dfs' stack is not expected to grow to
    // more than 1000 elements (even when walking tens of millions of nodes).
    let dfs = Arc::new(Mutex::new(vec![Ipld::Link(root_cid)]));
    let mut to_be_fetched = vec![];

    // Loop until: No more items in `dfs` AND no running worker tasks.
    loop {
        while let Some(ipld) = lock_pop(&dfs) {
            {
                let mut dfs_guard = dfs.lock();
                // Scan for unseen CIDs. Available IPLD nodes are pushed to the depth-first-search
                // stack, unavailable nodes will be requested in worker tasks.
                for new_cid in ipld.iter().filter_map(&mut get_ipld_link) {
                    counter += 1;
                    if counter % 1_000 == 0 {
                        // set RUST_LOG=forest_filecoin::rpc::state_api=debug to enable these printouts.
                        tracing::debug!(
                                "Graph walk: CIDs: {counter}, Fetched: {fetched}, Failures: {failures}, dfs: {}, Concurrent: {}",
                                dfs_guard.len(), task_set.len()
                            );
                    }

                    if let Some(next_ipld) = db.get_cbor(&new_cid)? {
                        dfs_guard.push(next_ipld);
                        if let Some(car_tx) = &car_tx {
                            car_tx.send((
                                new_cid,
                                db.get(&new_cid)?.with_context(|| {
                                    format!("Failed to get cid {new_cid} from block store")
                                })?,
                            ))?;
                        }
                    } else {
                        to_be_fetched.push(new_cid);
                    }
                }
            }

            while let Some(cid) = to_be_fetched.pop() {
                if task_set.len() == MAX_CONCURRENT_REQUESTS {
                    if let Some(ret) = task_set.join_next().await {
                        handle_worker(&mut fetched, &mut failures, ret?)
                    }
                }
                task_set.spawn_blocking({
                    let network_send = network_send.clone();
                    let db = db.clone();
                    let dfs_vec = Arc::clone(&dfs);
                    let car_tx = car_tx.clone();
                    move || {
                        let (tx, rx) = flume::bounded(1);
                        network_send.send(NetworkMessage::BitswapRequest {
                            cid,
                            response_channel: tx,
                        })?;
                        // Bitswap requests do not fail. They are just ignored if no-one has
                        // the requested data. Here we arbitrary decide to only wait for
                        // REQUEST_TIMEOUT before judging that the data is unavailable.
                        let _ignore = rx.recv_timeout(REQUEST_TIMEOUT);

                        let new_ipld = db
                            .get_cbor::<Ipld>(&cid)?
                            .ok_or_else(|| anyhow::anyhow!("Request failed: {cid}"))?;
                        dfs_vec.lock().push(new_ipld);
                        if let Some(car_tx) = &car_tx {
                            car_tx.send((
                                cid,
                                db.get(&cid)?.with_context(|| {
                                    format!("Failed to get cid {cid} from block store")
                                })?,
                            ))?;
                        }

                        Ok(())
                    }
                });
            }
            tokio::task::yield_now().await;
        }
        if let Some(ret) = task_set.join_next().await {
            handle_worker(&mut fetched, &mut failures, ret?)
        } else {
            // We are out of work items (dfs) and all worker threads have finished, this means
            // the entire graph has been walked and fetched.
            break;
        }
    }

    drop(car_tx);
    if let Some(car_handle) = car_handle {
        car_handle.await??;
    }

    Ok(format!(
        "IPLD graph traversed! CIDs: {counter}, fetched: {fetched}, failures: {failures}."
    ))
}

// Convenience function for locking and popping a value out of a vector. If this function is
// inlined, the mutex guard isn't dropped early enough.
fn lock_pop<T>(mutex: &Mutex<Vec<T>>) -> Option<T> {
    mutex.lock().pop()
}
