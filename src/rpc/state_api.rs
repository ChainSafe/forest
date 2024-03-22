// Copyright 2019-2024 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(clippy::unused_async)]

use crate::cid_collections::CidHashSet;
use crate::libp2p::NetworkMessage;
use crate::lotus_json::LotusJson;
use crate::rpc::error::JsonRpcError;
use crate::rpc::types::*;
use crate::rpc::Ctx;
use crate::shim::{
    address::Address, clock::ChainEpoch, deal::DealID, econ::TokenAmount, executor::Receipt,
    state_tree::ActorState, version::NetworkVersion,
};
use crate::state_manager::chain_rand::ChainRand;
use crate::state_manager::vm_circ_supply::GenesisInfo;
use crate::state_manager::{InvocResult, MarketBalance};
use crate::utils::db::car_stream::{CarBlock, CarWriter};
use ahash::{HashMap, HashMapExt};
use anyhow::Context as _;
use anyhow::Result;
use cid::Cid;
use fil_actor_interface::market::DealState;
use fil_actor_interface::miner::DeadlineInfo;
use fil_actor_interface::{
    market, miner,
    miner::{MinerInfo, MinerPower},
    multisig, power,
};
use fil_actors_shared::fvm_ipld_bitfield::BitField;
use futures::StreamExt;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::{CborStore, DAG_CBOR};
use jsonrpsee::types::{error::ErrorObject, Params};
use libipld_core::ipld::Ipld;
use nonempty::{nonempty, NonEmpty};
use num_bigint::BigInt;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::{sync::Arc, time::Duration};
use tokio::task::JoinSet;

type RandomnessParams = (i64, ChainEpoch, Vec<u8>, ApiTipsetKey);

pub async fn miner_get_base_info<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> anyhow::Result<LotusJson<Option<MiningBaseInfo>>, JsonRpcError> {
    let LotusJson((address, epoch, ApiTipsetKey(tsk))) = params.parse()?;

    let ts = data
        .state_manager
        .chain_store()
        .load_required_tipset_or_heaviest(&tsk)?;

    data.state_manager
        .miner_get_base_info(data.state_manager.beacon_schedule(), ts, address, epoch)
        .await
        .map(|info| Ok(LotusJson(info)))?
}
/// runs the given message and returns its result without any persisted changes.
pub async fn state_call<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<ApiInvocResult, JsonRpcError> {
    let LotusJson((message, ApiTipsetKey(key))) = params.parse()?;

    let state_manager = &data.state_manager;
    let tipset = data
        .state_manager
        .chain_store()
        .load_required_tipset_or_heaviest(&key)?;
    // Handle expensive fork error?
    // TODO(elmattic): https://github.com/ChainSafe/forest/issues/3733
    Ok(state_manager.call(&message, Some(tipset))?)
}

/// returns the result of executing the indicated message, assuming it was
/// executed in the indicated tipset.
pub async fn state_replay<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<InvocResult, JsonRpcError> {
    let LotusJson((cid, ApiTipsetKey(key))) = params.parse()?;

    let state_manager = &data.state_manager;
    let tipset = data
        .state_manager
        .chain_store()
        .load_required_tipset_or_heaviest(&key)?;
    let (msg, ret) = state_manager.replay(&tipset, cid).await?;

    Ok(InvocResult {
        msg,
        msg_rct: Some(ret.msg_receipt()),
        error: ret.failure_info(),
    })
}

/// gets network name from state manager
pub async fn state_network_name<DB: Blockstore>(data: Ctx<DB>) -> Result<String, JsonRpcError> {
    let state_manager = &data.state_manager;
    let heaviest_tipset = state_manager.chain_store().heaviest_tipset();

    state_manager
        .get_network_name(heaviest_tipset.parent_state())
        .map_err(|e| e.into())
}

pub async fn state_get_network_version<DB: Blockstore>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<NetworkVersion, JsonRpcError> {
    let LotusJson((ApiTipsetKey(tsk),)): LotusJson<(ApiTipsetKey,)> = params.parse()?;

    let ts = data.chain_store.load_required_tipset_or_heaviest(&tsk)?;
    Ok(data.state_manager.get_network_version(ts.epoch()))
}

/// gets the public key address of the given ID address
/// See <https://github.com/filecoin-project/lotus/blob/master/documentation/en/api-v0-methods.md#StateAccountKey>
pub async fn state_account_key<DB: Blockstore>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<Address>, JsonRpcError>
where
    DB: Blockstore + Send + Sync + 'static,
{
    let LotusJson((address, tipset_keys)): LotusJson<(Address, ApiTipsetKey)> = params.parse()?;

    let ts = data
        .chain_store
        .load_required_tipset_or_heaviest(&tipset_keys.0)?;
    Ok(LotusJson(
        data.state_manager
            .resolve_to_deterministic_address(address, ts)
            .await?,
    ))
}

/// retrieves the ID address of the given address
/// See <https://github.com/filecoin-project/lotus/blob/master/documentation/en/api-v0-methods.md#StateLookupID>
pub async fn state_lookup_id<DB: Blockstore>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<Address>, JsonRpcError>
where
    DB: Blockstore + Send + Sync + 'static,
{
    let LotusJson((address, tipset_keys)): LotusJson<(Address, ApiTipsetKey)> = params.parse()?;

    let ts = data
        .chain_store
        .load_required_tipset_or_heaviest(&tipset_keys.0)?;
    let ret = data
        .state_manager
        .lookup_id(&address, ts.as_ref())?
        .with_context(|| format!("Failed to lookup the id address for address: {address} and tipset keys: {tipset_keys}"))?;
    Ok(LotusJson(ret))
}

pub(crate) async fn state_get_actor<DB: Blockstore>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<Option<ActorState>>, JsonRpcError> {
    let LotusJson((addr, ApiTipsetKey(tsk))): LotusJson<(Address, ApiTipsetKey)> =
        params.parse()?;

    let ts = data.chain_store.load_required_tipset_or_heaviest(&tsk)?;
    let state = data.state_manager.get_actor(&addr, *ts.parent_state());
    state.map(Into::into).map_err(|e| e.into())
}

/// looks up the Escrow and Locked balances of the given address in the Storage
/// Market
pub async fn state_market_balance<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<MarketBalance, JsonRpcError> {
    let LotusJson((address, ApiTipsetKey(key))): LotusJson<(Address, ApiTipsetKey)> =
        params.parse()?;

    let tipset = data
        .state_manager
        .chain_store()
        .load_required_tipset_or_heaviest(&key)?;
    data.state_manager
        .market_balance(&address, &tipset)
        .map_err(|e| e.into())
}

pub async fn state_market_deals<DB: Blockstore>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<HashMap<String, MarketDeal>, JsonRpcError> {
    let LotusJson((ApiTipsetKey(tsk),)): LotusJson<(ApiTipsetKey,)> = params.parse()?;

    let ts = data.chain_store.load_required_tipset_or_heaviest(&tsk)?;
    let actor = data
        .state_manager
        .get_actor(&Address::MARKET_ACTOR, *ts.parent_state())?
        .context("Market actor address could not be resolved")?;
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
            verified_claim: 0,
        });
        out.insert(
            deal_id.to_string(),
            MarketDeal {
                proposal: d?,
                state: s,
            },
        );
        Ok(())
    })?;
    Ok(out)
}

/// looks up the miner info of the given address.
pub async fn state_miner_info<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<MinerInfo>, JsonRpcError> {
    let LotusJson((address, ApiTipsetKey(key))): LotusJson<(Address, ApiTipsetKey)> =
        params.parse()?;

    let tipset = data
        .state_manager
        .chain_store()
        .load_required_tipset_or_heaviest(&key)?;
    Ok(LotusJson(data.state_manager.miner_info(&address, &tipset)?))
}

pub async fn state_miner_active_sectors<DB: Blockstore>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<Vec<SectorOnChainInfo>>, JsonRpcError> {
    let LotusJson((miner, ApiTipsetKey(tsk))): LotusJson<(Address, ApiTipsetKey)> =
        params.parse()?;

    let bs = data.state_manager.blockstore();
    let ts = data.chain_store.load_required_tipset_or_heaviest(&tsk)?;
    let policy = &data.state_manager.chain_config().policy;
    let actor = data
        .state_manager
        .get_actor(&miner, *ts.parent_state())?
        .context("Miner actor address could not be resolved")?;
    let miner_state = miner::State::load(bs, actor.code, actor.state)?;

    // Collect active sectors from each partition in each deadline.
    let mut active_sectors = vec![];
    miner_state.for_each_deadline(policy, bs, |_dlidx, deadline| {
        deadline.for_each(bs, |_partidx, partition| {
            active_sectors.push(partition.active_sectors());
            Ok(())
        })
    })?;

    let sectors = miner_state
        .load_sectors(bs, Some(&BitField::union(&active_sectors)))?
        .into_iter()
        .map(SectorOnChainInfo::from)
        .collect::<Vec<_>>();

    Ok(LotusJson(sectors))
}

// Returns the number of sectors in a miner's sector set and proving set
pub async fn state_miner_sector_count<DB: Blockstore>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<MinerSectors>, JsonRpcError> {
    let LotusJson((miner, ApiTipsetKey(tsk))): LotusJson<(Address, ApiTipsetKey)> =
        params.parse()?;

    let bs = data.state_manager.blockstore();
    let ts = data.chain_store.load_required_tipset_or_heaviest(&tsk)?;
    let policy = &data.state_manager.chain_config().policy;
    let actor = data
        .state_manager
        .get_actor(&miner, *ts.parent_state())?
        .context("Miner actor address could not be resolved")?;
    let miner_state = miner::State::load(bs, actor.code, actor.state)?;

    // Collect live, active and faulty sectors count from each partition in each deadline.
    let mut live_count = 0;
    let mut active_count = 0;
    let mut faulty_count = 0;
    miner_state.for_each_deadline(policy, bs, |_dlidx, deadline| {
        deadline.for_each(bs, |_partidx, partition| {
            live_count += partition.live_sectors().len();
            active_count += partition.active_sectors().len();
            faulty_count += partition.faulty_sectors().len();
            Ok(())
        })
    })?;
    Ok(LotusJson(MinerSectors::new(
        live_count,
        active_count,
        faulty_count,
    )))
}

/// looks up the miner power of the given address.
pub async fn state_miner_power<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<MinerPower>, JsonRpcError> {
    let LotusJson((address, ApiTipsetKey(key))): LotusJson<(Address, ApiTipsetKey)> =
        params.parse()?;

    let tipset = data
        .state_manager
        .chain_store()
        .load_required_tipset_or_heaviest(&key)?;

    data.state_manager
        .miner_power(&address, &tipset)
        .map(|res| res.into())
        .map_err(|e| e.into())
}

pub async fn state_miner_deadlines<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<Vec<ApiDeadline>>, JsonRpcError> {
    let LotusJson((addr, ApiTipsetKey(tsk))): LotusJson<(Address, ApiTipsetKey)> =
        params.parse()?;

    let ts = data.chain_store.load_required_tipset_or_heaviest(&tsk)?;
    let policy = &data.state_manager.chain_config().policy;
    let actor = data
        .state_manager
        .get_actor(&addr, *ts.parent_state())?
        .context("Miner actor address could not be resolved")?;
    let store = data.state_manager.blockstore();
    let state = miner::State::load(store, actor.code, actor.state)?;
    let mut res = Vec::new();
    state.for_each_deadline(policy, store, |_idx, deadline| {
        res.push(ApiDeadline {
            post_submissions: deadline.partitions_posted(),
            disputable_proof_count: deadline.disputable_proof_count(store)?,
        });
        Ok(())
    })?;
    Ok(LotusJson(res))
}

pub async fn state_miner_proving_deadline<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<DeadlineInfo>, JsonRpcError> {
    let LotusJson((addr, ApiTipsetKey(tsk))): LotusJson<(Address, ApiTipsetKey)> =
        params.parse()?;

    let ts = data.chain_store.load_required_tipset_or_heaviest(&tsk)?;
    let policy = &data.state_manager.chain_config().policy;
    let actor = data
        .state_manager
        .get_actor(&addr, *ts.parent_state())?
        .context("Miner actor address could not be resolved")?;
    let store = data.state_manager.blockstore();
    let state = miner::State::load(store, actor.code, actor.state)?;
    Ok(LotusJson(state.deadline_info(policy, ts.epoch())))
}

/// looks up the miner power of the given address.
pub async fn state_miner_faults<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<BitField>, JsonRpcError> {
    let LotusJson((address, ApiTipsetKey(key))): LotusJson<(Address, ApiTipsetKey)> =
        params.parse()?;

    let ts = data
        .state_manager
        .chain_store()
        .load_required_tipset_or_heaviest(&key)?;

    data.state_manager
        .miner_faults(&address, &ts)
        .map_err(|e| e.into())
        .map(|r| r.into())
}

pub async fn state_miner_recoveries<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<BitField>, JsonRpcError> {
    let LotusJson((miner, ApiTipsetKey(tsk))): LotusJson<(Address, ApiTipsetKey)> =
        params.parse()?;

    let ts = data
        .state_manager
        .chain_store()
        .load_required_tipset_or_heaviest(&tsk)?;

    data.state_manager
        .miner_recoveries(&miner, &ts)
        .map_err(|e| e.into())
        .map(|r| r.into())
}

pub async fn state_miner_available_balance<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<TokenAmount>, JsonRpcError> {
    let LotusJson((miner_address, ApiTipsetKey(tsk))): LotusJson<(Address, ApiTipsetKey)> =
        params.parse()?;

    let store = data.chain_store.blockstore();
    let ts = data
        .state_manager
        .chain_store()
        .load_required_tipset_or_heaviest(&tsk)?;
    let actor = data
        .state_manager
        .get_actor(&miner_address, *ts.parent_state())?
        .ok_or_else(|| anyhow::anyhow!("Miner actor not found"))?;
    let state = miner::State::load(store, actor.code, actor.state)?;
    let actor_balance: TokenAmount = actor.balance.clone().into();
    let (vested, available): (TokenAmount, TokenAmount) = match &state {
        miner::State::V13(s) => (
            s.check_vested_funds(store, ts.epoch())?.into(),
            s.get_available_balance(&actor_balance.into())?.into(),
        ),
        miner::State::V12(s) => (
            s.check_vested_funds(store, ts.epoch())?.into(),
            s.get_available_balance(&actor_balance.into())?.into(),
        ),
        miner::State::V11(s) => (
            s.check_vested_funds(store, ts.epoch())?.into(),
            s.get_available_balance(&actor_balance.into())?.into(),
        ),
        miner::State::V10(s) => (
            s.check_vested_funds(store, ts.epoch())?.into(),
            s.get_available_balance(&actor_balance.into())?.into(),
        ),
        miner::State::V9(s) => (
            s.check_vested_funds(store, ts.epoch())?.into(),
            s.get_available_balance(&actor_balance.into())?.into(),
        ),
        miner::State::V8(s) => (
            s.check_vested_funds(store, ts.epoch())?.into(),
            s.get_available_balance(&actor_balance.into())?.into(),
        ),
    };

    Ok(LotusJson(vested + available))
}

/// returns the message receipt for the given message
pub async fn state_get_receipt<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<Receipt>, JsonRpcError> {
    let LotusJson((cid, ApiTipsetKey(key))): LotusJson<(Cid, ApiTipsetKey)> = params.parse()?;

    let state_manager = &data.state_manager;
    let tipset = data
        .state_manager
        .chain_store()
        .load_required_tipset_or_heaviest(&key)?;
    state_manager
        .get_receipt(tipset, cid)
        .map(|s| s.into())
        .map_err(|e| e.into())
}
/// looks back in the chain for a message. If not found, it blocks until the
/// message arrives on chain, and gets to the indicated confidence depth.
pub async fn state_wait_msg<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<MessageLookup, JsonRpcError> {
    let LotusJson((cid, confidence)): LotusJson<(Cid, i64)> = params.parse()?;

    let state_manager = &data.state_manager;
    let (tipset, receipt) = state_manager.wait_for_message(cid, confidence).await?;
    let tipset = tipset.context("wait for msg returned empty tuple")?;
    let receipt = receipt.context("wait for msg returned empty receipt")?;
    let ipld = receipt.return_data().deserialize().unwrap_or(Ipld::Null);

    Ok(MessageLookup {
        receipt,
        tipset: tipset.key().clone(),
        height: tipset.epoch(),
        message: cid,
        return_dec: ipld,
    })
}

/// Searches for a message in the chain, and returns its receipt and the tipset where it was executed.
/// See <https://github.com/filecoin-project/lotus/blob/master/documentation/en/api-v0-methods.md#StateSearchMsg>
pub async fn state_search_msg<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<MessageLookup, JsonRpcError> {
    let LotusJson((cid,)): LotusJson<(Cid,)> = params.parse()?;

    let state_manager = &data.state_manager;
    let (tipset, receipt) = state_manager
        .search_for_message(None, cid, None)
        .await?
        .with_context(|| format!("message {cid} not found."))?;

    let ipld = receipt.return_data().deserialize().unwrap_or(Ipld::Null);

    Ok(MessageLookup {
        receipt,
        tipset: tipset.key().clone(),
        height: tipset.epoch(),
        message: cid,
        return_dec: ipld,
    })
}

/// Looks back up to limit epochs in the chain for a message, and returns its receipt and the tipset where it was executed.
/// See <https://github.com/filecoin-project/lotus/blob/master/documentation/en/api-v0-methods.md#StateSearchMsgLimited>
pub async fn state_search_msg_limited<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<MessageLookup, JsonRpcError> {
    let LotusJson((cid, look_back_limit)): LotusJson<(Cid, i64)> = params.parse()?;

    let state_manager = &data.state_manager;
    let (tipset, receipt) = state_manager
        .search_for_message(None, cid, Some(look_back_limit))
        .await?
        .with_context(|| {
            format!("message {cid} not found within the last {look_back_limit} epochs")
        })?;

    let ipld = receipt.return_data().deserialize().unwrap_or(Ipld::Null);

    Ok(MessageLookup {
        receipt,
        tipset: tipset.key().clone(),
        height: tipset.epoch(),
        message: cid,
        return_dec: ipld,
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
pub async fn state_fetch_root<DB: Blockstore + Sync + Send + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<String, JsonRpcError> {
    let LotusJson((root_cid, save_to_file)): LotusJson<(Cid, Option<PathBuf>)> = params.parse()?;

    let network_send = data.network_send.clone();
    let db = data.chain_store.db.clone();
    drop(data);

    let (car_tx, car_handle) = if let Some(save_to_file) = save_to_file {
        let (car_tx, car_rx) = flume::bounded(100);
        let roots = nonempty![root_cid];
        let file = tokio::fs::File::create(save_to_file).await?;

        let car_handle = tokio::spawn(async move {
            car_rx
                .stream()
                .map(Ok)
                .forward(CarWriter::new_carv1(roots, file)?)
                .await
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
                            car_tx.send(CarBlock {
                                cid: new_cid,
                                data: db.get(&new_cid)?.with_context(|| {
                                    format!("Failed to get cid {new_cid} from block store")
                                })?,
                            })?;
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
                            epoch: None,
                        })?;
                        // Bitswap requests do not fail. They are just ignored if no-one has
                        // the requested data. Here we arbitrary decide to only wait for
                        // REQUEST_TIMEOUT before judging that the data is unavailable.
                        let _ignore = rx.recv_timeout(REQUEST_TIMEOUT);

                        let new_ipld = db
                            .get_cbor::<Ipld>(&cid)?
                            .with_context(|| format!("Request failed: {cid}"))?;
                        dfs_vec.lock().push(new_ipld);
                        if let Some(car_tx) = &car_tx {
                            car_tx.send(CarBlock {
                                cid,
                                data: db.get(&cid)?.with_context(|| {
                                    format!("Failed to get cid {cid} from block store")
                                })?,
                            })?;
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

/// Get randomness from tickets
pub async fn state_get_randomness_from_tickets<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<Vec<u8>>, JsonRpcError> {
    let LotusJson((personalization, rand_epoch, entropy, ApiTipsetKey(tsk))): LotusJson<
        RandomnessParams,
    > = params.parse()?;

    let state_manager = &data.state_manager;
    let tipset = state_manager
        .chain_store()
        .load_required_tipset_or_heaviest(&tsk)?;
    let chain_config = state_manager.chain_config();
    let chain_index = &data.chain_store.chain_index;
    let beacon = state_manager.beacon_schedule();
    let chain_rand = ChainRand::new(chain_config.clone(), tipset, chain_index.clone(), beacon);
    let digest = chain_rand.get_chain_randomness(rand_epoch, false)?;
    let value = crate::state_manager::chain_rand::draw_randomness_from_digest(
        &digest,
        personalization,
        rand_epoch,
        &entropy,
    )?;
    Ok(LotusJson(value.to_vec()))
}

/// Get randomness from beacon
pub async fn state_get_randomness_from_beacon<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<Vec<u8>>, JsonRpcError> {
    let LotusJson((personalization, rand_epoch, entropy, ApiTipsetKey(tsk))): LotusJson<
        RandomnessParams,
    > = params.parse()?;

    let state_manager = &data.state_manager;
    let tipset = state_manager
        .chain_store()
        .load_required_tipset_or_heaviest(&tsk)?;
    let chain_config = state_manager.chain_config();
    let chain_index = &data.chain_store.chain_index;
    let beacon = state_manager.beacon_schedule();
    let chain_rand = ChainRand::new(chain_config.clone(), tipset, chain_index.clone(), beacon);
    let digest = chain_rand.get_beacon_randomness_v3(rand_epoch)?;
    let value = crate::state_manager::chain_rand::draw_randomness_from_digest(
        &digest,
        personalization,
        rand_epoch,
        &entropy,
    )?;
    Ok(LotusJson(value.to_vec()))
}

/// Get read state
pub async fn state_read_state<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<ApiActorState>, JsonRpcError> {
    let LotusJson((addr, ApiTipsetKey(tsk))) = params.parse()?;

    let ts = data.chain_store.load_required_tipset_or_heaviest(&tsk)?;
    let actor = data
        .state_manager
        .get_actor(&addr, *ts.parent_state())?
        .context("Actor address could not be resolved")?;
    let blk = data
        .state_manager
        .blockstore()
        .get(&actor.state)?
        .context("Failed to get block from blockstore")?;
    let state = *fvm_ipld_encoding::from_slice::<NonEmpty<Cid>>(&blk)?.first();

    Ok(LotusJson(ApiActorState::new(
        actor.balance.clone().into(),
        actor.code,
        Ipld::Link(state),
    )))
}

pub async fn state_circulating_supply<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<TokenAmount>, JsonRpcError> {
    let LotusJson((ApiTipsetKey(tsk),)) = params.parse()?;

    let ts = data.chain_store.load_required_tipset_or_heaviest(&tsk)?;

    let height = ts.epoch();

    let state_manager = &data.state_manager;

    let root = ts.parent_state();

    let genesis_info = GenesisInfo::from_chain_config(state_manager.chain_config());

    let supply =
        genesis_info.get_circulating_supply(height, &state_manager.blockstore_owned(), root)?;

    Ok(LotusJson(supply))
}

pub async fn msig_get_available_balance<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<TokenAmount>, JsonRpcError> {
    let LotusJson((addr, ApiTipsetKey(tsk))) = params.parse()?;

    let ts = data.chain_store.load_required_tipset_or_heaviest(&tsk)?;
    let height = ts.epoch();
    let store = data.state_manager.blockstore();
    let actor = data
        .state_manager
        .get_actor(&addr, *ts.parent_state())?
        .context("MultiSig actor not found")?;
    let actor_balance = TokenAmount::from(&actor.balance);
    let ms = multisig::State::load(&store, actor.code, actor.state)?;
    let locked_balance = ms.locked_balance(height)?.into();
    let avail_balance = &actor_balance - locked_balance;
    Ok(LotusJson(avail_balance))
}

pub async fn msig_get_pending<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<Vec<Transaction>>, JsonRpcError> {
    let LotusJson((addr, ApiTipsetKey(tsk))) = params.parse()?;

    let ts = data.chain_store.load_required_tipset_or_heaviest(&tsk)?;
    let store = data.state_manager.blockstore();
    let actor = data
        .state_manager
        .get_actor(&addr, *ts.parent_state())?
        .context("MultiSig actor not found")?;
    let ms = multisig::State::load(&store, actor.code, actor.state)?;
    let txns = ms
        .get_pending_txn(store)?
        .iter()
        .map(|txn| Transaction {
            id: txn.id,
            to: txn.to.into(),
            value: txn.value.clone().into(),
            method: txn.method,
            params: txn.params.clone(),
            approved: txn.approved.iter().map(|item| item.into()).collect(),
        })
        .collect();

    Ok(LotusJson(txns))
}

/// Get state sector info using sector no
pub async fn state_sector_get_info<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<SectorOnChainInfo>, JsonRpcError> {
    let LotusJson((addr, sector_no, ApiTipsetKey(tsk))): LotusJson<(Address, u64, ApiTipsetKey)> =
        params.parse()?;

    let ts = data.chain_store.load_required_tipset_or_heaviest(&tsk)?;

    Ok(LotusJson(
        data.state_manager
            .get_all_sectors(&addr, &ts)?
            .into_iter()
            .find(|info| info.sector_number == sector_no)
            .map(SectorOnChainInfo::from)
            .context(format!("Info for sector number {sector_no} not found"))?,
    ))
}

pub(in crate::rpc) async fn state_verified_client_status<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<Option<BigInt>>, JsonRpcError> {
    let LotusJson((addr, ApiTipsetKey(tsk))) = params.parse()?;

    let ts = data.chain_store.load_required_tipset_or_heaviest(&tsk)?;
    let status = data.state_manager.verified_client_status(&addr, &ts)?;
    Ok(status.into())
}

pub(in crate::rpc) async fn state_vm_circulating_supply_internal<
    DB: Blockstore + Send + Sync + 'static,
>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<CirculatingSupply>, JsonRpcError> {
    let LotusJson((ApiTipsetKey(tsk),)) = params.parse()?;

    let ts = data.chain_store.load_required_tipset_or_heaviest(&tsk)?;

    let genesis_info = GenesisInfo::from_chain_config(data.state_manager.chain_config());

    Ok(LotusJson(genesis_info.get_vm_circulating_supply_detailed(
        ts.epoch(),
        &data.state_manager.blockstore_owned(),
        ts.parent_state(),
    )?))
}

/// Looks back and returns all messages with a matching to or from address, stopping at the given height.
pub(in crate::rpc) async fn state_list_messages<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<Vec<Cid>>, JsonRpcError> {
    let LotusJson((from_to, tsk, max_height)): LotusJson<(MessageFilter, ApiTipsetKey, i64)> =
        params.parse()?;

    let ts = data.chain_store.load_required_tipset_or_heaviest(&tsk.0)?;

    if from_to.is_empty() {
        return Err(ErrorObject::owned(
            1,
            "must specify at least To or From in message filter",
            Some(from_to),
        )
        .into());
    } else if let Some(to) = from_to.to {
        // this is following lotus logic, it probably should be `if let` instead of `else if let`
        // see <https://github.com/ChainSafe/forest/pull/3827#discussion_r1462691005>
        data.state_manager
            .lookup_id(&to, ts.as_ref())?
            .with_context(|| {
                format!("Failed to lookup the id address for address: {to} and tipset keys: {tsk}")
            })?;
    } else if let Some(from) = from_to.from {
        data.state_manager
            .lookup_id(&from, ts.as_ref())?
            .with_context(|| {
                format!(
                    "Failed to lookup the id address for address: {from} and tipset keys: {tsk}"
                )
            })?;
    }

    let mut out = Vec::new();
    let mut cur_ts = ts.clone();

    while cur_ts.epoch() >= max_height {
        let msgs = data.chain_store.messages_for_tipset(&cur_ts)?;

        for msg in msgs {
            if from_to.matches(msg.message()) {
                out.push(msg.cid()?);
            }
        }

        if cur_ts.epoch() == 0 {
            break;
        }

        let next = data
            .state_manager
            .chain_store()
            .chain_index
            .load_tipset(cur_ts.parents())?
            .context("failed to load next tipset")?;
        cur_ts = next;
    }

    Ok(LotusJson(out))
}

pub async fn state_list_miners<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<LotusJson<Vec<Address>>, JsonRpcError> {
    let LotusJson((ApiTipsetKey(tsk),)) = params.parse()?;

    let ts = data
        .state_manager
        .chain_store()
        .load_required_tipset_or_heaviest(&tsk)?;
    let store = data.state_manager.blockstore();
    let actor = data
        .state_manager
        .get_actor(&Address::POWER_ACTOR, *ts.parent_state())?
        .context("Power actor not found")?;

    let state = power::State::load(store, actor.code, actor.state)?;
    let miners = state
        .list_all_miners(store)?
        .iter()
        .map(|addr| addr.into())
        .collect();

    Ok(LotusJson(miners))
}

pub async fn state_market_storage_deal<DB: Blockstore + Send + Sync + 'static>(
    params: Params<'_>,
    data: Ctx<DB>,
) -> Result<ApiMarketDeal, JsonRpcError> {
    let LotusJson((deal_id, ApiTipsetKey(tsk))): LotusJson<(DealID, ApiTipsetKey)> =
        params.parse()?;

    let ts = data
        .state_manager
        .chain_store()
        .load_required_tipset_or_heaviest(&tsk)?;
    let store = data.state_manager.blockstore();
    let actor = data
        .state_manager
        .get_actor(&Address::MARKET_ACTOR, *ts.parent_state())?
        .context("Market actor not found")?;
    let market_state = market::State::load(store, actor.code, actor.state)?;
    let proposals = market_state.proposals(store)?;
    let proposal =  proposals.get(deal_id)?.ok_or_else(|| anyhow::anyhow!("deal {deal_id} not found - deal may not have completed sealing before deal proposal start epoch, or deal may have been slashed"))?;

    let states = market_state.states(store)?;
    let state = states.get(deal_id)?.unwrap_or_else(DealState::empty);

    Ok(MarketDeal { proposal, state }.into())
}
