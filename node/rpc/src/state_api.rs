// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use jsonrpc_v2::{Data, Error as JsonRpcError, Params};
use std::collections::HashMap;

use cid::Cid;
use forest_actor_interface::{market, power, reward};
use forest_beacon::Beacon;
use forest_blocks::tipset_keys_json::TipsetKeysJson;
use forest_db::Store;
use forest_ipld::json::IpldJson;
use forest_json::address::json::AddressJson;
use forest_json::cid::CidJson;
use forest_rpc_api::{
    data_types::{MarketDeal, MessageLookup, RPCState},
    state_api::*,
};
use forest_state_manager::InvocResult;
use fvm::state_tree::StateTree;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared::bigint::BigInt;
use fvm_shared::econ::TokenAmount;
use libipld_core::ipld::Ipld;

// TODO handle using configurable verification implementation in RPC (all defaulting to Full).

/// runs the given message and returns its result without any persisted changes.
pub(crate) async fn state_call<
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateCallParams>,
) -> Result<StateCallResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (message_json, key) = params;
    let mut message = message_json.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    Ok(state_manager.call(&mut message, Some(tipset)).await?)
}

/// returns the result of executing the indicated message, assuming it was executed in the indicated tipset.
pub(crate) async fn state_replay<
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateReplayParams>,
) -> Result<StateReplayResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let (cidjson, key) = params;
    let cid = cidjson.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    let (msg, ret) = state_manager.replay(&tipset, cid).await?;

    Ok(InvocResult {
        msg,
        msg_rct: Some(ret.msg_receipt),
        error: ret.failure_info.map(|e| e.to_string()),
    })
}

/// gets network name from state manager
pub(crate) async fn state_network_name<
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
>(
    data: Data<RPCState<DB, B>>,
) -> Result<StateNetworkNameResult, JsonRpcError> {
    let state_manager = &data.state_manager;
    let heaviest_tipset = state_manager
        .chain_store()
        .heaviest_tipset()
        .await
        .ok_or("Heaviest Tipset not found in state_network_name")?;

    state_manager
        .get_network_name(heaviest_tipset.parent_state())
        .map_err(|e| e.into())
}

pub(crate) async fn state_get_network_version<
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateNetworkVersionParams>,
) -> Result<StateNetworkVersionResult, JsonRpcError> {
    let (TipsetKeysJson(tsk),) = params;
    let ts = data.chain_store.tipset_from_keys(&tsk).await?;
    Ok(data.state_manager.get_network_version(ts.epoch()))
}

/// looks up the Escrow and Locked balances of the given address in the Storage Market
pub(crate) async fn state_market_balance<
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
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
        .tipset_from_keys(&key.into())
        .await?;
    data.state_manager
        .market_balance(&address, &tipset)
        .map_err(|e| e.into())
}

pub(crate) async fn state_market_deals<
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateMarketDealsParams>,
) -> Result<StateMarketDealsResult, JsonRpcError> {
    let (TipsetKeysJson(tsk),) = params;
    let ts = data.chain_store.tipset_from_keys(&tsk).await?;
    let actor = data
        .state_manager
        .get_actor(&market::ADDRESS, *ts.parent_state())?
        .ok_or("Market actor address could not be resolved")?;
    let market_state = market::State::load(data.state_manager.blockstore(), &actor)?;

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
pub(crate) async fn state_get_receipt<
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateGetReceiptParams>,
) -> Result<StateGetReceiptResult, JsonRpcError> {
    let (cidjson, key) = params;
    let state_manager = &data.state_manager;
    let cid = cidjson.into();
    let tipset = data
        .state_manager
        .chain_store()
        .tipset_from_keys(&key.into())
        .await?;
    state_manager
        .get_receipt(&tipset, cid)
        .await
        .map(|s| s.into())
        .map_err(|e| e.into())
}
/// looks back in the chain for a message. If not found, it blocks until the
/// message arrives on chain, and gets to the indicated confidence depth.
pub(crate) async fn state_wait_msg<
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateWaitMsgParams>,
) -> Result<StateWaitMsgResult, JsonRpcError> {
    let (cidjson, confidence) = params;
    let state_manager = &data.state_manager;
    let cid: Cid = cidjson.into();
    let (tipset, receipt) = state_manager.wait_for_message(cid, confidence).await?;
    let tipset = tipset.ok_or("wait for msg returned empty tuple")?;
    let receipt = receipt.ok_or("wait for msg returned empty receipt")?;
    let ipld: Ipld = if receipt.return_data.bytes().is_empty() {
        Ipld::Null
    } else {
        receipt.return_data.deserialize()?
    };
    Ok(MessageLookup {
        receipt: receipt.into(),
        tipset: tipset.key().clone().into(),
        height: tipset.epoch(),
        message: CidJson(cid),
        return_dec: IpldJson(ipld),
    })
}

pub(crate) async fn state_miner_pre_commit_deposit_for_power<
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateMinerPreCommitDepositForPowerParams>,
) -> Result<StateMinerPreCommitDepositForPowerResult, JsonRpcError> {
    let (AddressJson(maddr), pci, TipsetKeysJson(tsk)) = params;
    let ts = data.chain_store.tipset_from_keys(&tsk).await?;
    let (state, _) = data.state_manager.tipset_state(&ts).await?;
    let state = StateTree::new_from_root(&data.chain_store.db, &state)?;
    let ssize = pci.seal_proof.sector_size()?;

    let actor = state
        .get_actor(&market::ADDRESS)?
        .ok_or("couldnt load market actor")?;
    let (w, vw) = market::State::load(data.state_manager.blockstore(), &actor)?
        .verify_deals_for_activation(
            data.state_manager.blockstore(),
            &pci.deal_ids,
            &maddr,
            pci.expiration,
            ts.epoch(),
        )?;
    let duration = pci.expiration - ts.epoch();
    let sector_weight = fil_actor_miner_v8::qa_power_for_weight(ssize, duration, &w, &vw);

    let actor = state
        .get_actor(&power::ADDRESS)?
        .ok_or("couldnt load power actor")?;
    let power_smoothed =
        power::State::load(data.state_manager.blockstore(), &actor)?.total_power_smoothed();

    let reward_actor = state
        .get_actor(&reward::ADDRESS)?
        .ok_or("couldnt load reward actor")?;
    let deposit = reward::State::load(data.state_manager.blockstore(), &reward_actor)?
        .pre_commit_deposit_for_power(power_smoothed, &sector_weight);

    let ret: TokenAmount = (deposit * 110).div_floor(100);
    Ok(ret.to_string())
}

pub(crate) async fn state_miner_initial_pledge_collateral<
    DB: Blockstore + Store + Clone + Send + Sync + 'static,
    B: Beacon,
>(
    data: Data<RPCState<DB, B>>,
    Params(params): Params<StateMinerInitialPledgeCollateralParams>,
) -> Result<StateMinerInitialPledgeCollateralResult, JsonRpcError> {
    let (AddressJson(maddr), pci, TipsetKeysJson(tsk)) = params;
    let ts = data.chain_store.tipset_from_keys(&tsk).await?;
    let (root_cid, _) = data.state_manager.tipset_state(&ts).await?;
    let state = StateTree::new_from_root(&data.chain_store.db, &root_cid)?;
    let ssize = pci.seal_proof.sector_size()?;

    let actor = state
        .get_actor(&market::ADDRESS)?
        .ok_or("couldnt load market actor")?;
    let (w, vw) = market::State::load(data.state_manager.blockstore(), &actor)?
        .verify_deals_for_activation(
            data.state_manager.blockstore(),
            &pci.deal_ids,
            &maddr,
            pci.expiration,
            ts.epoch(),
        )?;
    let duration = pci.expiration - ts.epoch();
    let sector_weight = fil_actor_miner_v8::qa_power_for_weight(ssize, duration, &w, &vw);

    let actor = state
        .get_actor(&power::ADDRESS)?
        .ok_or("couldnt load power actor")?;
    let power_state = power::State::load(data.state_manager.blockstore(), &actor)?;
    let power_smoothed = power_state.total_power_smoothed();
    let total_locked = power_state.total_locked();

    let circ_supply =
        data.state_manager
            .get_circulating_supply(ts.epoch(), &data.chain_store.db, &root_cid)?;

    let reward_actor = state
        .get_actor(&reward::ADDRESS)?
        .ok_or("couldnt load reward actor")?;

    let initial_pledge = reward::State::load(data.state_manager.blockstore(), &reward_actor)?
        .initial_pledge_for_power(&sector_weight, &total_locked, power_smoothed, &circ_supply);

    let ret: BigInt = (initial_pledge.atto() * 110) / 100;
    Ok(ret.to_string())
}
