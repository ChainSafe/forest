// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use ahash::{HashMap, HashMapExt, HashSet, HashSetExt};
use cid::Cid;
use fil_actor_interface::market;
use forest_beacon::Beacon;
use forest_blocks::tipset_keys_json::TipsetKeysJson;
use forest_ipld::json::IpldJson;
use forest_json::cid::CidJson;
use forest_libp2p::NetworkMessage;
use forest_rpc_api::{
    data_types::{MarketDeal, MessageLookup, RPCState},
    state_api::*,
};
use forest_shim::address::Address;
use forest_state_manager::InvocResult;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{CborStore, DAG_CBOR};
use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use libipld_core::ipld::Ipld;
use std::{sync::Arc, time::Duration};
use tokio::{sync::Semaphore, task::JoinSet, time::timeout};

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
pub(crate) async fn state_fetch_root<DB: Blockstore + Clone + Sync + Send + 'static, B: Beacon>(
    data: Data<RPCState<DB, B>>,
    Params((CidJson(root_cid),)): Params<StateFetchRootParams>,
) -> Result<StateFetchRootResult, JsonRpcError> {
    const MAX_CONCURRENT_REQUESTS: usize = 16;
    const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

    let sem = Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS));
    let mut seen: HashSet<Cid> = HashSet::new();
    let mut counter: usize = 0;
    let mut failures: usize = 0;
    let mut task_set = JoinSet::new();

    let mut get_ipld_link = |ipld: &Ipld| match ipld {
        Ipld::Link(cid) if cid.codec() == DAG_CBOR && seen.insert(*cid) => Some(*cid),
        _ => None,
    };

    task_set.spawn(async move { Ok(Ipld::Link(root_cid)) });

    // Iterate until there are no more ipld nodes to traverse
    while let Some(result) = task_set.join_next().await {
        match result? {
            Ok(ipld) => {
                for new_cid in ipld.iter().filter_map(&mut get_ipld_link) {
                    counter += 1;
                    if counter % 1_000 == 0 {
                        // set RUST_LOG=forest_rpc::state_api=debug to enable these printouts.
                        log::debug!(
                            "Still downloading. Fetched: {counter}, Failures: {failures}, Concurrent: {}",
                            MAX_CONCURRENT_REQUESTS - sem.available_permits()
                        );
                    }
                    task_set.spawn({
                        let network_send = data.network_send.clone();
                        let db = data.chain_store.db.clone();
                        let sem = sem.clone();
                        async move {
                            if !db.has(&new_cid)? {
                                // If a CID isn't in our database, request it via bitswap (limited
                                // by MAX_CONCURRENT_REQUESTS)
                                let permit = sem.acquire_owned().await?;
                                let (tx, rx) = flume::bounded(1);
                                network_send
                                    .send_async(NetworkMessage::BitswapRequest {
                                        epoch: 0,
                                        cid: new_cid,
                                        response_channel: tx,
                                    })
                                    .await?;
                                // Bitswap requests do not fail. They are just ignored if no-one has
                                // the requested data. Here we arbitrary decide to only wait for
                                // REQUEST_TIMEOUT before deciding that the data is unavailable.
                                let _ignore = timeout(REQUEST_TIMEOUT, rx.recv_async()).await;
                                drop(permit);
                            }

                            db.get_cbor::<Ipld>(&new_cid)?
                                .ok_or_else(|| anyhow::anyhow!("Request failed: {new_cid}"))
                        }
                    });
                }
            }
            Err(msg) => {
                failures += 1;
                log::debug!("Request failed: {msg}");
            }
        }
    }
    Ok(format!(
        "IPLD graph traversed! CIDs: {counter}, failures: {failures}."
    ))
}
