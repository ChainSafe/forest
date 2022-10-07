// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use anyhow::Context;
use forest_actor_interface::{
    market, power, reward, BURNT_FUNDS_ACTOR_ADDR, EPOCHS_IN_DAY, RESERVE_ADDRESS,
};
use forest_chain::*;
use forest_interpreter::CircSupplyCalc;
use forest_ipld_blockstore::BlockStore;
use forest_networks::{ChainConfig, Height};
use fvm::state_tree::{ActorState, StateTree};
use fvm_shared::address::Address;
use fvm_shared::bigint::BigInt;
use fvm_shared::bigint::Integer;
use fvm_shared::clock::ChainEpoch;
use fvm_shared::econ::TokenAmount;
use fvm_shared::FILECOIN_PRECISION;
use once_cell::sync::OnceCell;

const EPOCHS_IN_YEAR: ChainEpoch = 365 * EPOCHS_IN_DAY;
const PRE_CALICO_VESTING: [(ChainEpoch, usize); 5] = [
    (183 * EPOCHS_IN_DAY, 82_717_041),
    (EPOCHS_IN_YEAR, 22_421_712),
    (2 * EPOCHS_IN_YEAR, 7_223_364),
    (3 * EPOCHS_IN_YEAR, 87_637_883),
    (6 * EPOCHS_IN_YEAR, 400_000_000),
];
const CALICO_VESTING: [(ChainEpoch, usize); 6] = [
    (0, 10_632_000),
    (183 * EPOCHS_IN_DAY, 19_015_887 + 32_787_700),
    (EPOCHS_IN_YEAR, 22_421_712 + 9_400_000),
    (2 * EPOCHS_IN_YEAR, 7_223_364),
    (3 * EPOCHS_IN_YEAR, 87_637_883 + 898_958),
    (6 * EPOCHS_IN_YEAR, 100_000_000 + 300_000_000 + 9_805_053),
];

/// Genesis information used when calculating circulating supply.
#[derive(Default, Clone)]
pub(crate) struct GenesisInfo {
    vesting: GenesisInfoVesting,

    /// info about the Accounts in the genesis state
    genesis_pledge: OnceCell<TokenAmount>,
    genesis_market_funds: OnceCell<TokenAmount>,

    /// Heights epoch
    ignition_height: ChainEpoch,
    actors_v2_height: ChainEpoch,
    liftoff_height: ChainEpoch,
    calico_height: ChainEpoch,
}

impl GenesisInfo {
    pub fn from_chain_config(chain_config: &ChainConfig) -> Self {
        let ignition_height = chain_config.epoch(Height::Ignition);
        let actors_v2_height = chain_config.epoch(Height::ActorsV2);
        let liftoff_height = chain_config.epoch(Height::Liftoff);
        let calico_height = chain_config.epoch(Height::Calico);
        Self {
            ignition_height,
            actors_v2_height,
            liftoff_height,
            calico_height,
            ..GenesisInfo::default()
        }
    }

    fn init<DB: BlockStore>(&self, _bs: &DB) -> Result<(), anyhow::Error> {
        Ok(())
    }
}

/// Vesting schedule info. These states are lazily filled, to avoid doing until needed
/// to calculate circulating supply.
#[derive(Default, Clone)]
struct GenesisInfoVesting {
    genesis: OnceCell<Vec<(ChainEpoch, TokenAmount)>>,
    ignition: OnceCell<Vec<(ChainEpoch, ChainEpoch, TokenAmount)>>,
    calico: OnceCell<Vec<(ChainEpoch, ChainEpoch, TokenAmount)>>,
}

impl CircSupplyCalc for GenesisInfo {
    fn get_supply<DB: BlockStore>(
        &self,
        height: ChainEpoch,
        state_tree: &StateTree<DB>,
    ) -> Result<TokenAmount, anyhow::Error> {
        // TODO investigate a better way to handle initializing the genesis actors rather than
        // on first circ supply call. This is currently necessary because it is how Lotus does it
        // but it's not ideal to have the side effect from the VM to modify the genesis info
        // of the state manager. This isn't terrible because it's just caching to avoid
        // recalculating using the store, and it avoids computing until circ_supply is called.
        self.vesting
            .genesis
            .get_or_try_init(|| -> Result<_, anyhow::Error> {
                self.init(state_tree.store())?;
                Ok(setup_genesis_vesting_schedule())
            })?;

        self.vesting
            .ignition
            .get_or_init(|| setup_ignition_vesting_schedule(self.liftoff_height));

        self.vesting
            .calico
            .get_or_init(|| setup_calico_vesting_schedule(self.liftoff_height));

        get_circulating_supply(self, height, state_tree)
    }
}

fn get_actor_state<DB: BlockStore>(
    state_tree: &StateTree<DB>,
    addr: &Address,
) -> Result<ActorState, anyhow::Error> {
    state_tree
        .get_actor(addr)?
        .ok_or_else(|| anyhow::anyhow!("Failed to get Actor for address {}", addr))
}

fn get_fil_vested(genesis_info: &GenesisInfo, height: ChainEpoch) -> TokenAmount {
    let mut return_value = TokenAmount::default();

    let pre_ignition = genesis_info
        .vesting
        .genesis
        .get()
        .expect("Pre ignition should be initialized");
    let post_ignition = genesis_info
        .vesting
        .ignition
        .get()
        .expect("Post ignition should be initialized");
    let calico_vesting = genesis_info
        .vesting
        .calico
        .get()
        .expect("calico vesting should be initialized");

    if height <= genesis_info.ignition_height {
        for (unlock_duration, initial_balance) in pre_ignition {
            return_value +=
                initial_balance - v0_amount_locked(*unlock_duration, initial_balance, height);
        }
    } else if height <= genesis_info.calico_height {
        for (start_epoch, unlock_duration, initial_balance) in post_ignition {
            return_value += initial_balance
                - v0_amount_locked(*unlock_duration, initial_balance, height - start_epoch);
        }
    } else {
        for (start_epoch, unlock_duration, initial_balance) in calico_vesting {
            return_value += initial_balance
                - v0_amount_locked(*unlock_duration, initial_balance, height - start_epoch);
        }
    }

    if height <= genesis_info.actors_v2_height {
        return_value += genesis_info
            .genesis_pledge
            .get()
            .expect("Genesis info should be initialized")
            + genesis_info
                .genesis_market_funds
                .get()
                .expect("Genesis info should be initialized");
    }

    return_value
}

fn get_fil_mined<DB: BlockStore>(state_tree: &StateTree<DB>) -> Result<TokenAmount, anyhow::Error> {
    let actor = state_tree
        .get_actor(&reward::ADDRESS)?
        .context("Reward actor address could not be resolved")?;
    let state = reward::State::load(state_tree.store(), &actor)?;

    Ok(state.into_total_storage_power_reward())
}

fn get_fil_market_locked<DB: BlockStore>(
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, anyhow::Error> {
    let actor = state_tree
        .get_actor(&market::ADDRESS)?
        .ok_or_else(|| Error::State("Market actor address could not be resolved".to_string()))?;
    let state = market::State::load(state_tree.store(), &actor)?;

    Ok(state.total_locked())
}

fn get_fil_power_locked<DB: BlockStore>(
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, anyhow::Error> {
    let actor = state_tree
        .get_actor(&power::ADDRESS)?
        .ok_or_else(|| Error::State("Power actor address could not be resolved".to_string()))?;
    let state = power::State::load(state_tree.store(), &actor)?;

    Ok(state.into_total_locked())
}

fn get_fil_reserve_disbursed<DB: BlockStore>(
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, anyhow::Error> {
    let fil_reserved: BigInt = BigInt::from(300_000_000) * FILECOIN_PRECISION;
    let reserve_actor = get_actor_state(state_tree, &RESERVE_ADDRESS)?;

    // If money enters the reserve actor, this could lead to a negative term
    Ok(fil_reserved - reserve_actor.balance)
}

fn get_fil_locked<DB: BlockStore>(
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, anyhow::Error> {
    let market_locked = get_fil_market_locked(state_tree)?;
    let power_locked = get_fil_power_locked(state_tree)?;
    Ok(power_locked + market_locked)
}

fn get_fil_burnt<DB: BlockStore>(state_tree: &StateTree<DB>) -> Result<TokenAmount, anyhow::Error> {
    let burnt_actor = get_actor_state(state_tree, BURNT_FUNDS_ACTOR_ADDR)?;

    Ok(burnt_actor.balance)
}

fn get_circulating_supply<DB: BlockStore>(
    genesis_info: &GenesisInfo,
    height: ChainEpoch,
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, anyhow::Error> {
    let fil_vested = get_fil_vested(genesis_info, height);
    let fil_mined = get_fil_mined(state_tree)?;
    let fil_burnt = get_fil_burnt(state_tree)?;
    let fil_locked = get_fil_locked(state_tree)?;
    let fil_reserve_distributed = if height > genesis_info.actors_v2_height {
        get_fil_reserve_disbursed(state_tree)?
    } else {
        TokenAmount::default()
    };
    let fil_circulating = BigInt::max(
        &fil_vested + &fil_mined + &fil_reserve_distributed - &fil_burnt - &fil_locked,
        TokenAmount::default(),
    );

    Ok(fil_circulating)
}

fn setup_genesis_vesting_schedule() -> Vec<(ChainEpoch, TokenAmount)> {
    PRE_CALICO_VESTING
        .into_iter()
        .map(|(unlock_duration, initial_balance)| {
            (unlock_duration, TokenAmount::from(initial_balance))
        })
        .collect()
}

fn setup_ignition_vesting_schedule(
    liftoff_height: ChainEpoch,
) -> Vec<(ChainEpoch, ChainEpoch, TokenAmount)> {
    PRE_CALICO_VESTING
        .into_iter()
        .map(|(unlock_duration, initial_balance)| {
            (
                liftoff_height,
                unlock_duration,
                TokenAmount::from(initial_balance) * FILECOIN_PRECISION,
            )
        })
        .collect()
}
fn setup_calico_vesting_schedule(
    liftoff_height: ChainEpoch,
) -> Vec<(ChainEpoch, ChainEpoch, TokenAmount)> {
    CALICO_VESTING
        .into_iter()
        .map(|(unlock_duration, initial_balance)| {
            (
                liftoff_height,
                unlock_duration,
                TokenAmount::from(initial_balance) * FILECOIN_PRECISION,
            )
        })
        .collect()
}

// This exact code (bugs and all) has to be used. The results are locked into the blockchain.
/// Returns amount locked in multisig contract
fn v0_amount_locked(
    unlock_duration: ChainEpoch,
    initial_balance: &TokenAmount,
    elapsed_epoch: ChainEpoch,
) -> TokenAmount {
    if elapsed_epoch >= unlock_duration {
        return TokenAmount::from(0);
    }
    if elapsed_epoch < 0 {
        return initial_balance.clone();
    }
    // Division truncation is broken here: https://github.com/filecoin-project/specs-actors/issues/1131
    let unit_locked: TokenAmount = initial_balance.div_floor(&TokenAmount::from(unlock_duration));
    unit_locked * (unlock_duration - elapsed_epoch)
}
