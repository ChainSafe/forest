// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use actor::*;
use actorv0::multisig as msig0;
use address::Address;
use blockstore::BlockStore;
use chain::*;
use cid::Cid;
use clock::ChainEpoch;
use fil_types::{FILECOIN_PRECISION, FIL_RESERVED};
use interpreter::CircSupplyCalc;
use networks::{
    UPGRADE_ACTORS_V2_HEIGHT, UPGRADE_CALICO_HEIGHT, UPGRADE_IGNITION_HEIGHT,
    UPGRADE_LIFTOFF_HEIGHT,
};
use num_bigint::BigInt;
use once_cell::sync::OnceCell;
use state_tree::StateTree;
use std::error::Error as StdError;
use vm::{ActorState, TokenAmount};

const EPOCHS_IN_YEAR: ChainEpoch = 365 * actor::EPOCHS_IN_DAY;

lazy_static! {
    static ref PRE_CALICO_VESTING: [(ChainEpoch, TokenAmount); 5] = [
        (183 * actor::EPOCHS_IN_DAY, TokenAmount::from(82_717_041)),
        (EPOCHS_IN_YEAR, TokenAmount::from(22_421_712)),
        (2 * EPOCHS_IN_YEAR, TokenAmount::from(7_223_364)),
        (3 * EPOCHS_IN_YEAR, TokenAmount::from(87_637_883)),
        (6 * EPOCHS_IN_YEAR, TokenAmount::from(400_000_000)),
    ];
    static ref CALICO_VESTING: [(ChainEpoch, TokenAmount); 6] = [
        (0, TokenAmount::from(10_632_000)),
        (
            183 * actor::EPOCHS_IN_DAY,
            TokenAmount::from(19_015_887 + 32_787_700)
        ),
        (EPOCHS_IN_YEAR, TokenAmount::from(22_421_712 + 9_400_000)),
        (2 * EPOCHS_IN_YEAR, TokenAmount::from(7_223_364)),
        (3 * EPOCHS_IN_YEAR, TokenAmount::from(87_637_883 + 898_958)),
        (
            6 * EPOCHS_IN_YEAR,
            TokenAmount::from(100_000_000 + 300_000_000 + 9_805_053)
        ),
    ];
}

/// Genesis information used when calculating circulating supply.
#[derive(Default)]
pub(crate) struct GenesisInfo {
    vesting: GenesisInfoVesting,

    /// info about the Accounts in the genesis state
    genesis_pledge: OnceCell<TokenAmount>,
    genesis_market_funds: OnceCell<TokenAmount>,
}

impl GenesisInfo {
    fn init<DB: BlockStore>(&self, bs: &DB) -> Result<(), Box<dyn StdError>> {
        let genesis_block =
            genesis(bs)?.ok_or_else(|| "Genesis Block doesn't exist".to_string())?;

        // Parent state of genesis tipset is tipset state
        let st = genesis_block.state_root();

        let state_tree = StateTree::new_from_root(bs, st)?;

        let _ = self
            .genesis_market_funds
            .set(get_fil_market_locked(&state_tree)?);
        let _ = self.genesis_pledge.set(get_fil_power_locked(&state_tree)?);

        Ok(())
    }
}

/// Vesting schedule info. These states are lazily filled, to avoid doing until needed
/// to calculate circulating supply.
#[derive(Default)]
struct GenesisInfoVesting {
    genesis: OnceCell<Vec<msig0::State>>,
    ignition: OnceCell<Vec<msig0::State>>,
    calico: OnceCell<Vec<msig0::State>>,
}

impl CircSupplyCalc for GenesisInfo {
    fn get_supply<DB: BlockStore>(
        &self,
        height: ChainEpoch,
        state_tree: &StateTree<DB>,
    ) -> Result<TokenAmount, Box<dyn StdError>> {
        // TODO investigate a better way to handle initializing the genesis actors rather than
        // on first circ supply call. This is currently necessary because it is how Lotus does it
        // but it's not ideal to have the side effect from the VM to modify the genesis info
        // of the state manager. This isn't terrible because it's just caching to avoid
        // recalculating using the store, and it avoids computing until circ_supply is called.
        self.vesting
            .genesis
            .get_or_try_init(|| -> Result<_, Box<dyn StdError>> {
                self.init(state_tree.store())?;
                Ok(setup_genesis_vesting_schedule())
            })?;

        self.vesting
            .ignition
            .get_or_init(setup_ignition_vesting_schedule);

        self.vesting
            .calico
            .get_or_init(setup_calico_vesting_schedule);

        get_circulating_supply(self, height, state_tree)
    }
}

fn get_actor_state<DB: BlockStore>(
    state_tree: &StateTree<DB>,
    addr: &Address,
) -> Result<ActorState, Box<dyn StdError>> {
    Ok(state_tree
        .get_actor(addr)?
        .ok_or_else(|| format!("Failed to get Actor for address {}", addr))?)
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

    if height <= UPGRADE_IGNITION_HEIGHT {
        for actor in pre_ignition {
            return_value += &actor.initial_balance - actor.amount_locked(height);
        }
    } else if height <= UPGRADE_CALICO_HEIGHT {
        for actor in post_ignition {
            return_value +=
                &actor.initial_balance - actor.amount_locked(height - actor.start_epoch);
        }
    } else {
        for actor in calico_vesting {
            return_value +=
                &actor.initial_balance - actor.amount_locked(height - actor.start_epoch);
        }
    }

    if height <= UPGRADE_ACTORS_V2_HEIGHT {
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

fn get_fil_mined<DB: BlockStore>(
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, Box<dyn StdError>> {
    let actor = state_tree
        .get_actor(reward::ADDRESS)?
        .ok_or_else(|| Error::State("Power actor address could not be resolved".to_string()))?;
    let state = reward::State::load(state_tree.store(), &actor)?;

    Ok(state.into_total_storage_power_reward())
}

fn get_fil_market_locked<DB: BlockStore>(
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, Box<dyn StdError>> {
    let actor = state_tree
        .get_actor(market::ADDRESS)?
        .ok_or_else(|| Error::State("Power actor address could not be resolved".to_string()))?;
    let state = market::State::load(state_tree.store(), &actor)?;

    Ok(state.total_locked())
}

fn get_fil_power_locked<DB: BlockStore>(
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, Box<dyn StdError>> {
    let actor = state_tree
        .get_actor(power::ADDRESS)?
        .ok_or_else(|| Error::State("Power actor address could not be resolved".to_string()))?;
    let state = power::State::load(state_tree.store(), &actor)?;

    Ok(state.into_total_locked())
}

fn get_fil_reserve_disbursed<DB: BlockStore>(
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, Box<dyn StdError>> {
    let reserve_actor = get_actor_state(state_tree, RESERVE_ADDRESS)?;

    // If money enters the reserve actor, this could lead to a negative term
    Ok(&*FIL_RESERVED - reserve_actor.balance)
}

fn get_fil_locked<DB: BlockStore>(
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, Box<dyn StdError>> {
    let market_locked = get_fil_market_locked(state_tree)?;
    let power_locked = get_fil_power_locked(state_tree)?;
    Ok(power_locked + market_locked)
}

fn get_fil_burnt<DB: BlockStore>(
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, Box<dyn StdError>> {
    let burnt_actor = get_actor_state(state_tree, &*BURNT_FUNDS_ACTOR_ADDR)?;

    Ok(burnt_actor.balance)
}

fn get_circulating_supply<'a, DB: BlockStore>(
    genesis_info: &GenesisInfo,
    height: ChainEpoch,
    state_tree: &StateTree<'a, DB>,
) -> Result<TokenAmount, Box<dyn StdError>> {
    let fil_vested = get_fil_vested(genesis_info, height);
    let fil_mined = get_fil_mined(state_tree)?;
    let fil_burnt = get_fil_burnt(state_tree)?;
    let fil_locked = get_fil_locked(state_tree)?;
    let fil_reserve_distributed = if height > UPGRADE_ACTORS_V2_HEIGHT {
        get_fil_reserve_disbursed(state_tree)?
    } else {
        TokenAmount::default()
    };
    let fil_circulating = BigInt::max(
        &fil_vested + &fil_mined + fil_reserve_distributed - &fil_burnt - &fil_locked,
        TokenAmount::default(),
    );

    Ok(fil_circulating)
}

fn setup_genesis_vesting_schedule() -> Vec<msig0::State> {
    PRE_CALICO_VESTING
        .iter()
        .map(|(unlock_duration, initial_balance)| {
            msig0::State {
                signers: vec![],
                num_approvals_threshold: 0,
                next_tx_id: msig0::TxnID(0),
                initial_balance: initial_balance.clone(),
                start_epoch: ChainEpoch::default(),
                unlock_duration: *unlock_duration,
                // Default Cid is ok here because this field is never read
                pending_txs: Cid::default(),
            }
        })
        .collect()
}

fn setup_ignition_vesting_schedule() -> Vec<msig0::State> {
    PRE_CALICO_VESTING
        .iter()
        .map(|(unlock_duration, initial_balance)| {
            msig0::State {
                signers: vec![],
                num_approvals_threshold: 0,
                next_tx_id: msig0::TxnID(0),

                // In the pre-ignition logic, this value was incorrectly set in Fil, not attoFil,
                // an off-by-10^18 error
                initial_balance: initial_balance * FILECOIN_PRECISION,

                // In the pre-ignition logic, the start epoch was 0. This changes in the fork logic
                // of the Ignition upgrade itself.
                start_epoch: UPGRADE_LIFTOFF_HEIGHT,

                unlock_duration: *unlock_duration,
                // Default Cid is ok here because this field is never read
                pending_txs: Cid::default(),
            }
        })
        .collect()
}

fn setup_calico_vesting_schedule() -> Vec<msig0::State> {
    CALICO_VESTING
        .iter()
        .map(|(unlock_duration, initial_balance)| {
            msig0::State {
                signers: vec![],
                num_approvals_threshold: 0,
                next_tx_id: msig0::TxnID(0),
                initial_balance: initial_balance * FILECOIN_PRECISION,
                start_epoch: UPGRADE_LIFTOFF_HEIGHT,
                unlock_duration: *unlock_duration,
                // Default Cid is ok here because this field is never read
                pending_txs: Cid::default(),
            }
        })
        .collect()
}
