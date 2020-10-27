// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::*;
use address::Address;
use blockstore::BlockStore;
use chain::*;
use cid::Cid;
use clock::ChainEpoch;
use fil_types::{
    FILECOIN_PRECISION, FIL_RESERVED, UPGRADE_ACTORS_V2_HEIGHT, UPGRADE_IGNITION_HEIGHT,
    UPGRADE_LIFTOFF_HEIGHT,
};
use forest_blocks::Tipset;
use interpreter::CircSupplyCalc;
use num_bigint::BigInt;
use state_tree::StateTree;
use std::error::Error as StdError;

lazy_static! {
    static ref TOTALS_BY_EPOCH: Vec<(ChainEpoch, TokenAmount)> = {
        let epoch_in_year = 365 * network::EPOCHS_IN_DAY;
        vec![
            (183 * network::EPOCHS_IN_DAY, TokenAmount::from(82_717_041)),
            (epoch_in_year, TokenAmount::from(22_421_712)),
            (2 * epoch_in_year, TokenAmount::from(7_223_364)),
            (3 * epoch_in_year, TokenAmount::from(87_637_883)),
            (6 * epoch_in_year, TokenAmount::from(400_000_000)),
        ]
    };
}

pub struct GenesisActor {
    addr: Address,
    init_bal: TokenAmount,
}

#[derive(Default)]
pub struct GenesisInfo {
    genesis_msigs: Vec<multisig::State>,
    /// info about the Accounts in the genesis state
    genesis_actors: Vec<GenesisActor>,
    genesis_pledge: TokenAmount,
    genesis_market_funds: TokenAmount,
}

#[derive(Default)]
pub struct GenesisInfoPair {
    pub pre_ignition: GenesisInfo,
    pub post_ignition: GenesisInfo,
}

impl CircSupplyCalc for GenesisInfoPair {
    fn get_supply<DB: BlockStore>(
        &self,
        height: ChainEpoch,
        state_tree: &StateTree<DB>,
    ) -> Result<TokenAmount, Box<dyn StdError>> {
        return get_circulating_supply(&self.pre_ignition, &self.post_ignition, height, state_tree);
    }
}

fn get_actor_state<DB: BlockStore>(
    state_tree: &StateTree<DB>,
    addr: &Address,
) -> Result<ActorState, Box<dyn StdError>> {
    Ok(state_tree
        .get_actor(&addr)?
        .ok_or_else(|| "Failed to get Actor ")?)
}

pub fn get_fil_vested<DB: BlockStore>(
    pre_ignition: &GenesisInfo,
    post_ignition: &GenesisInfo,
    height: ChainEpoch,
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, Box<dyn StdError>> {
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
        let state = get_actor_state(state_tree, &actor.addr)?;
        let diff = &actor.init_bal - state.balance;
        if diff > TokenAmount::default() {
            return_value += diff
        }
    }

    if height <= UPGRADE_ACTORS_V2_HEIGHT {
        return_value += &pre_ignition.genesis_pledge + &pre_ignition.genesis_market_funds;
    }

    Ok(return_value)
}

pub fn get_fil_mined<DB: BlockStore>(
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, Box<dyn StdError>> {
    let reward_actor = get_actor_state(state_tree, &*REWARD_ACTOR_ADDR)?;
    let reward_state: reward::State = state_tree
        .store()
        .get(&reward_actor.state)?
        .ok_or_else(|| "Failed to get Rewrad Actor State".to_string())?;

    Ok(reward_state.total_storage_power_reward().clone())
}

pub fn get_fil_market_locked<DB: BlockStore>(
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, Box<dyn StdError>> {
    let market_actor = get_actor_state(state_tree, &*STORAGE_MARKET_ACTOR_ADDR)?;

    let market_state: market::State = state_tree
        .store()
        .get(&market_actor.state)?
        .ok_or_else(|| "Failed to get Market Actor State".to_string())?;

    Ok(market_state.total_locked())
}

pub fn get_fil_power_locked<DB: BlockStore>(
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, Box<dyn StdError>> {
    let power_actor = get_actor_state(state_tree, &*STORAGE_POWER_ACTOR_ADDR)?;

    let power_state: power::State = state_tree
        .store()
        .get(&power_actor.state)?
        .ok_or_else(|| "Failed to get Power Actor State".to_string())?;

    Ok(power_state.total_locked().clone())
}

pub fn get_fil_reserve_disbursed<DB: BlockStore>(
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, Box<dyn StdError>> {
    let reserve_actor = get_actor_state(state_tree, &RESERVE_ADDRESS)?;

    // If money enters the reserve actor, this could lead to a negative term
    Ok(&*FIL_RESERVED - reserve_actor.balance)
}

pub fn get_fil_locked<DB: BlockStore>(
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, Box<dyn StdError>> {
    let market_locked = get_fil_market_locked(&state_tree)?;
    let power_locked = get_fil_power_locked(&state_tree)?;
    Ok(power_locked + market_locked)
}

pub fn get_fil_burnt<DB: BlockStore>(
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, Box<dyn StdError>> {
    let burnt_actor = get_actor_state(state_tree, &*BURNT_FUNDS_ACTOR_ADDR)?;

    Ok(burnt_actor.balance)
}

pub fn get_circulating_supply<'a, DB: BlockStore>(
    pre_ignition: &GenesisInfo,
    post_ignition: &GenesisInfo,
    height: ChainEpoch,
    state_tree: &StateTree<'a, DB>,
) -> Result<TokenAmount, Box<dyn StdError>> {
    let fil_vested = get_fil_vested(&pre_ignition, &post_ignition, height, &state_tree)?;
    let fil_mined = get_fil_mined(&state_tree)?;
    let fil_burnt = get_fil_burnt(&state_tree)?;
    let fil_locked = get_fil_locked(&state_tree)?;
    let fil_reserve_distributed = if height > UPGRADE_ACTORS_V2_HEIGHT {
        get_fil_reserve_disbursed(&state_tree)?
    } else {
        TokenAmount::default()
    };
    let fil_circulating = BigInt::max(
        &fil_vested + &fil_mined + fil_reserve_distributed - &fil_burnt - &fil_locked,
        TokenAmount::default(),
    );

    Ok(fil_circulating)
}

fn init_genesis_info<DB: BlockStore>(bs: &DB) -> Result<GenesisInfo, Box<dyn StdError>> {
    let mut ignition = GenesisInfo::default();

    let genesis_block = genesis(bs)?.ok_or_else(|| "Genesis Block doesnt exist".to_string())?;

    let gts = Tipset::new(vec![genesis_block])?;

    // Parent state of genesis tipset is tipset state
    let st = gts.parent_state();

    let state_tree = StateTree::new_from_root(bs, &st)?;

    ignition.genesis_market_funds = get_fil_market_locked(&state_tree)?;
    ignition.genesis_pledge = get_fil_power_locked(&state_tree)?;

    Ok(ignition)
}

pub fn setup_preignition_genesis_actors_testnet<DB: BlockStore>(
    bs: &DB,
) -> Result<GenesisInfo, Box<dyn StdError>> {
    let mut pre_ignition = init_genesis_info(bs)?;

    for (unlock_duration, initial_balance) in &*TOTALS_BY_EPOCH {
        let ms = multisig::State {
            signers: vec![],
            num_approvals_threshold: 0,
            next_tx_id: multisig::TxnID(0),
            initial_balance: initial_balance.clone(),
            start_epoch: ChainEpoch::default(),
            unlock_duration: *unlock_duration,
            // Default Cid is ok here because this field is never read
            pending_txs: Cid::default(),
        };
        pre_ignition.genesis_msigs.push(ms);
    }

    Ok(pre_ignition)
}

pub fn setup_postignition_genesis_actors_testnet<DB: BlockStore>(
    bs: &DB,
) -> Result<GenesisInfo, Box<dyn StdError>> {
    let mut post_ignition = init_genesis_info(bs)?;

    for (unlock_duration, initial_balance) in &*TOTALS_BY_EPOCH {
        let ms = multisig::State {
            signers: vec![],
            num_approvals_threshold: 0,
            next_tx_id: multisig::TxnID(0),
            initial_balance: initial_balance * FILECOIN_PRECISION,
            start_epoch: UPGRADE_LIFTOFF_HEIGHT,
            unlock_duration: *unlock_duration,
            // Default Cid is ok here because this field is never read
            pending_txs: Cid::default(),
        };
        post_ignition.genesis_msigs.push(ms);
    }

    Ok(post_ignition)
}
