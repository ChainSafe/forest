// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use ahash::{HashMap, HashMapExt};
use cid::Cid;
use fil_actor_interface::market;
use forest_beacon::Beacon;
use forest_blocks::tipset_keys_json::TipsetKeysJson;
use forest_ipld::json::IpldJson;
use forest_json::cid::CidJson;
use forest_rpc_api::{
    data_types::{MarketDeal, MessageLookup, RPCState},
    state_api::*,
};
use forest_shim::address::Address;
use forest_state_manager::InvocResult;
use fvm_ipld_blockstore::Blockstore;
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use libipld_core::ipld::Ipld;
use log::info;
use std::time::Duration;

// TODO handle using configurable verification implementation in RPC (all
// defaulting to Full).

/// runs the given message and returns its result without any persisted changes.
pub(crate) async fn state_call<DB: Blockstore + Clone + Send + Sync + 'static, B: Beacon>(
    data: Data<RPCState<DB, B>>,
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
pub(crate) async fn state_replay<DB: Blockstore + Clone + Send + Sync + 'static, B: Beacon>(
    data: Data<RPCState<DB, B>>,
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
pub(crate) async fn state_network_name<
    DB: Blockstore + Clone + Send + Sync + 'static,
    B: Beacon,
>(
    data: Data<RPCState<DB, B>>,
) -> Result<StateNetworkNameResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let heaviest_tipset = state_manager.chain_store().heaviest_tipset();

    state_manager
        .get_network_name(heaviest_tipset.parent_state())
        .map_err(|e| e.into())
}

pub(crate) async fn state_get_network_version<
    DB: Blockstore + Clone + Send + Sync + 'static,
    B: Beacon,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateNetworkVersionParams>,
) -> Result<StateNetworkVersionResult, JsonRpcError> {
    let (TipsetKeysJson(tsk),) = params;
    let ts = data.chain_store.tipset_from_keys(&tsk)?;
    Ok(data.state_manager.get_network_version(ts.epoch()))
}

/// looks up the Escrow and Locked balances of the given address in the Storage
/// Market
pub(crate) async fn state_market_balance<
    DB: Blockstore + Clone + Send + Sync + 'static,
    B: Beacon,
>(
    data: Data<RPCState<DB, B>>,
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

pub(crate) async fn state_market_deals<
    DB: Blockstore + Clone + Send + Sync + 'static,
    B: Beacon,
>(
    data: Data<RPCState<DB, B>>,
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
pub(crate) async fn state_get_receipt<DB: Blockstore + Clone + Send + Sync + 'static, B: Beacon>(
    data: Data<RPCState<DB, B>>,
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
pub(crate) async fn state_wait_msg<DB: Blockstore + Clone + Send + Sync + 'static, B: Beacon>(
    data: Data<RPCState<DB, B>>,
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

pub(crate) async fn state_fetch_root<DB: Blockstore + Sync + Send + 'static, B: Beacon>(
    data: Data<RPCState<DB, B>>,
    Params((CidJson(root_cid),)): Params<StateFetchRootParams>,
) -> Result<StateFetchRootResult, JsonRpcError> {
    {
        use ahash::{HashSet, HashSetExt};
        use forest_libp2p::NetworkMessage;
        use fvm_ipld_encoding::CborStore;
        use parking_lot::Mutex;
        use std::ops::DerefMut;
        use std::sync::Arc;

        fn scan_for_links(ipld: forest_ipld::Ipld, seen: &mut HashSet<Cid>, links: &mut Vec<Cid>) {
            match ipld {
                Ipld::Null => {}
                Ipld::Bool(_) => {}
                Ipld::Integer(_) => {}
                Ipld::Float(_) => {}
                Ipld::String(_) => {}
                Ipld::Bytes(_) => {}
                Ipld::List(list) => {
                    for elt in list.into_iter() {
                        scan_for_links(elt, seen, links);
                    }
                }
                Ipld::Map(map) => {
                    for (_key, elt) in map.into_iter() {
                        scan_for_links(elt, seen, links);
                    }
                }
                Ipld::Link(cid) => {
                    if cid.codec() == 0x55 {
                        if seen.insert(cid) {
                            // info!("Found WASM: {cid}");
                        }
                    }
                    if cid.codec() == 0x71 {
                        if seen.insert(cid) {
                            // info!("Found link: {cid}");
                            links.push(cid);
                        }
                    }
                }
            }
        }

        // tokio::time::sleep(Duration::from_secs(10)).await;

        let seen = Arc::new(Mutex::new(HashSet::new()));
        let sem = Arc::new(tokio::sync::Semaphore::new(16));
        let (work_send, mut work_recv) = tokio::sync::mpsc::channel(1024);
        let failures = Arc::new(Mutex::new(0_usize));
        let task_set = tokio::task::JoinSet::new();
        // mainnet: 1,594,681
        // let root_cid = "bafy2bzaceaclaz3jvmbjg3piazaq5dcesoyv26cdpoozlkzdiwnsvdvm2qoqm".parse::<Cid>().unwrap();

        // mainnet: 2,933,266
        // let root_cid = "bafy2bzacebyp6cmbshtzzuogzk7icf24pt6s5veyq5zkkqbn3sbbvswtptuuu".parse::<Cid>().unwrap();

        // mainnet: 2,833,266
        // let root_cid = "bafy2bzacecaydufxqo5vtouuysmg3tqik6onyuezm6lyviycriohgfnzfslm2".parse::<Cid>().unwrap();

        // mainnet: 1_960_320
        // let root_cid = "bafy2bzacec43okhmihmnwmgqspyrkuivqtxv75rpymsdbulq6lgsdq2vkwkcg".parse::<Cid>().unwrap();

        // calibnet: 242,150, 21144 cids
        // let root_cid = "bafy2bzaceb522vvt3wo7xhleo2dvb7wb7pyydmzlahc4aqd7lmvg3afreejiw".parse::<Cid>().unwrap();
        // calibnet: 630,932, 88594 cids
        // let root_cid = "bafy2bzacedidwdsd7ds73t3z76hcjfsaisoxrangkxsqlzih67ulqgtxnypqk".parse::<Cid>().unwrap();
        seen.lock().insert(root_cid);
        work_send.send(root_cid).await?;
        // to_fetch.push(root_cid);
        while let Some(required_cid) = work_recv.recv().await {
            info!(
                "Fetching new ipld block. Seen: {}, Failures: {}, Concurrent: {}",
                seen.lock().len(),
                failures.lock(),
                16 - sem.available_permits()
            );

            let permit = sem.clone().acquire_owned().await;
            tokio::task::spawn({
                let network_send = data.network_send.clone();
                let chain_store = data.chain_store.clone();
                let work_send = work_send.clone();
                let seen = seen.clone();
                let failures = failures.clone();
                async move {
                    let (tx, rx) = flume::bounded(1);
                    let _ignore = network_send
                        .send_async(NetworkMessage::BitswapRequest {
                            epoch: 0,
                            cid: required_cid,
                            response_channel: tx,
                        })
                        .await;

                    if !chain_store.db.has(&required_cid).unwrap_or(false) {
                        let _success = tokio::task::spawn_blocking(move || {
                            rx.recv_timeout(Duration::from_secs_f32(10.0))
                                .unwrap_or_default()
                        })
                        .await
                        .unwrap_or(false);
                    }
                    drop(permit);

                    match chain_store.db.get_cbor::<Ipld>(&required_cid) {
                        Ok(Some(ipld)) => {
                            // info!("Request successful");
                            let mut new_links = Vec::new();
                            scan_for_links(ipld, seen.lock().deref_mut(), &mut new_links);
                            for cid in new_links.into_iter() {
                                let _ignore_channel_close_errors = work_send.send(cid).await;
                            }
                            // forest_ipld::traverse_ipld_links_hash(seen, load_block, ipld, |_|);
                        }
                        Ok(None) => {
                            *failures.lock() += 1;
                            info!("Request failed: failures: {}", failures.lock())
                        }
                        Err(msg) => info!("Failed to decode data: {msg}"),
                    }
                    drop(work_send);
                }
            });

            // tokio::task::yield_now().await;
        }
        info!("All fetches done. Failures: {}", failures.lock());
        Ok(())
    }
}
