// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::*;
use address::Address;
use blocks::Tipset;
use chain::*;
use cid::Cid;
use clock::ChainEpoch;
use fil_types::{FILECOIN_PRECISION, UPGRADE_IGNITION_HEIGHT, UPGRADE_LIFTOFF_HEIGHT};
use ipld_blockstore::BlockStore;
use num_bigint::BigInt;
use state_tree::StateTree;
use std::collections::HashMap;

#[derive(Default)]
pub struct GenesisInfo {
    genesis_msigs: Vec<multisig::State>,
    // info about the Accounts in the genesis state
    genesis_actors: Vec<GenesisActor>,
    genesis_pledge: TokenAmount,
    genesis_market_funds: TokenAmount,
}

pub struct GenesisActor {
    addr: Address,
    init_bal: TokenAmount,
}

pub fn get_fil_vested<DB: BlockStore>(
    pre_ignition: &GenesisInfo,
    post_ignition: &GenesisInfo,
    height: ChainEpoch,
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, String> {
    let mut return_value = TokenAmount::default();

    if height <= UPGRADE_IGNITION_HEIGHT {
        for actor in &pre_ignition.genesis_msigs {
            return_value += &actor.initial_balance - actor.amount_locked(height);
        }
    } else {
        for actor in &post_ignition.genesis_msigs {
            return_value +=
                &actor.initial_balance - actor.amount_locked(height - actor.start_epoch);
        }
    }

    for actor in &pre_ignition.genesis_actors {
        let state = state_tree
            .get_actor(&actor.addr)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Failed to retreive ActorState".to_string())?;
        let diff = &actor.init_bal - state.balance;
        if diff > TokenAmount::default() {
            return_value += diff
        }
    }

    return_value += &pre_ignition.genesis_pledge + &pre_ignition.genesis_market_funds;
    Ok(return_value)
}

pub fn get_fil_mined<DB: BlockStore>(state_tree: &StateTree<DB>) -> Result<TokenAmount, String> {
    let reward_actor = state_tree
        .get_actor(&*REWARD_ACTOR_ADDR)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Failed to get Reward Actor".to_string())?;

    let reward_state: reward::State = state_tree
        .store()
        .get(&reward_actor.code)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Failed to get Rewrad Actor State".to_string())?;

    Ok(reward_state.total_storage_power_reward())
}

pub fn get_fil_market_locked<DB: BlockStore>(
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, String> {
    let market_actor = state_tree
        .get_actor(&*STORAGE_MARKET_ACTOR_ADDR)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Failed to get Market Actor".to_string())?;

    let market_state: market::State = state_tree
        .store()
        .get(&market_actor.state)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Failed to get Market Actor State".to_string())?;

    Ok(market_state.total_locked())
}

pub fn get_fil_power_locked<DB: BlockStore>(
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, String> {
    let power_actor = state_tree
        .get_actor(&*STORAGE_POWER_ACTOR_ADDR)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Failed to get Power Actor".to_string())?;

    let power_state: power::State = state_tree
        .store()
        .get(&power_actor.state)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Failed to get Power Actor State".to_string())?;

    Ok(power_state.total_locked())
}

pub fn get_fil_locked<DB: BlockStore>(state_tree: &StateTree<DB>) -> Result<TokenAmount, String> {
    let market_locked = get_fil_market_locked(&state_tree)?;
    let power_locked = get_fil_power_locked(&state_tree)?;
    Ok(power_locked + market_locked)
}

pub fn get_fil_burnt<DB: BlockStore>(state_tree: &StateTree<DB>) -> Result<TokenAmount, String> {
    let burnt_actor = state_tree
        .get_actor(&*BURNT_FUNDS_ACTOR_ADDR)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Failed to get Burnt Actor State".to_string())?;

    Ok(burnt_actor.balance)
}

pub fn get_circulating_supply<'a, DB: BlockStore>(
    pre_ignition: &GenesisInfo,
    post_ignition: &GenesisInfo,
    height: ChainEpoch,
    state_tree: &StateTree<'a, DB>,
) -> Result<TokenAmount, String> {
    let fil_vested = get_fil_vested(&pre_ignition, &post_ignition, height, &state_tree)?;
    let fil_mined = get_fil_mined(&state_tree)?;
    let fil_burnt = get_fil_burnt(&state_tree)?;
    let fil_locked = get_fil_locked(&state_tree)?;
    let fil_circulating = BigInt::max(
        &fil_vested + &fil_mined - &fil_burnt - &fil_locked,
        TokenAmount::default(),
    );

    Ok(fil_circulating)
}

fn get_totals_by_epoch() -> HashMap<ChainEpoch, TokenAmount> {
    let mut totals_by_epoch: HashMap<ChainEpoch, TokenAmount> = HashMap::new();

    let six_months = 183 * network::EPOCHS_IN_DAY;
    totals_by_epoch.insert(six_months, TokenAmount::from(82_717_041));

    let one_year = 365 * network::EPOCHS_IN_DAY;
    totals_by_epoch.insert(one_year, TokenAmount::from(22_421_712));

    let two_years = 2 * 365 * network::EPOCHS_IN_DAY;
    totals_by_epoch.insert(two_years, TokenAmount::from(7_223_364));

    let three_years = 3 * 365 * network::EPOCHS_IN_DAY;
    totals_by_epoch.insert(three_years, TokenAmount::from(87_637_883));

    let six_years = 6 * 365 * network::EPOCHS_IN_DAY;
    totals_by_epoch.insert(six_years, TokenAmount::from(400_000_000));

    totals_by_epoch
}

fn init_genesis_info<DB: BlockStore>(bs: &DB) -> Result<GenesisInfo, String> {
    let mut ignition = GenesisInfo::default();

    let genesis_block = genesis(bs)
        .map_err(|_| "Failed to get Genesis Block".to_string())?
        .ok_or_else(|| "Genesis Block doesnt exist".to_string())?;

    let gts =
        Tipset::new(vec![genesis_block]).map_err(|_| "Failed to get Genesis Tipset".to_string())?;

    // Parent state of genesis tipset is tipset state
    let st = gts.parent_state();

    let state_tree =
        StateTree::new_from_root(bs, &st).map_err(|_| "Failed to load state tree".to_string())?;

    ignition.genesis_market_funds = get_fil_market_locked(&state_tree)?;
    ignition.genesis_pledge = get_fil_power_locked(&state_tree)?;

    Ok(ignition)
}

pub fn setup_preignition_genesis_actors_testnet<DB: BlockStore>(
    bs: &DB,
) -> Result<GenesisInfo, String> {
    let mut pre_ignition = init_genesis_info(bs)?;

    let totals_by_epoch: HashMap<ChainEpoch, TokenAmount> = get_totals_by_epoch();

    for (unlock_duration, initial_balance) in totals_by_epoch {
        let ms = multisig::State {
            signers: vec![],
            num_approvals_threshold: 0,
            next_tx_id: multisig::TxnID(0),
            initial_balance,
            start_epoch: ChainEpoch::default(),
            unlock_duration,
            pending_txs: Cid::default(),
        };
        pre_ignition.genesis_msigs.push(ms);
    }

    Ok(pre_ignition)
}

pub fn setup_postignition_genesis_actors_testnet<DB: BlockStore>(
    bs: &DB,
) -> Result<GenesisInfo, String> {
    let mut post_ignition = init_genesis_info(bs)?;

    let totals_by_epoch: HashMap<ChainEpoch, TokenAmount> = get_totals_by_epoch();

    for (unlock_duration, initial_balance) in totals_by_epoch {
        let ms = multisig::State {
            signers: vec![],
            num_approvals_threshold: 0,
            next_tx_id: multisig::TxnID(0),
            initial_balance: initial_balance * FILECOIN_PRECISION,
            start_epoch: UPGRADE_LIFTOFF_HEIGHT,
            unlock_duration,
            pending_txs: Cid::default(),
        };
        post_ignition.genesis_msigs.push(ms);
    }

    Ok(post_ignition)
}
