// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::chain::*;
use crate::networks::{ChainConfig, Height};
use crate::rpc::types::CirculatingSupply;
use crate::shim::actors::{
    MarketActorStateLoad as _, MinerActorStateLoad as _, MultisigActorStateLoad as _,
    PowerActorStateLoad as _, is_account_actor, is_ethaccount_actor, is_evm_actor, is_miner_actor,
    is_multisig_actor, is_paymentchannel_actor, is_placeholder_actor,
};
use crate::shim::actors::{market, miner, multisig, power, reward};
use crate::shim::version::NetworkVersion;
use crate::shim::{
    address::Address,
    clock::{ChainEpoch, EPOCHS_IN_DAY},
    econ::{TOTAL_FILECOIN, TokenAmount},
    state_tree::{ActorState, StateTree},
};
use anyhow::{Context as _, bail};
use cid::Cid;
use fvm_ipld_blockstore::Blockstore;
use num_traits::Zero;

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
pub struct GenesisInfo {
    vesting: GenesisInfoVesting,

    /// info about the Accounts in the genesis state
    genesis_pledge: TokenAmount,
    genesis_market_funds: TokenAmount,

    chain_config: Arc<ChainConfig>,
}

impl GenesisInfo {
    pub fn from_chain_config(chain_config: Arc<ChainConfig>) -> Self {
        let liftoff_height = chain_config.epoch(Height::Liftoff);
        Self {
            vesting: GenesisInfoVesting::new(liftoff_height),
            chain_config,
            ..GenesisInfo::default()
        }
    }

    /// Calculate total FIL circulating supply based on Genesis configuration and state of particular
    /// actors at a given height and state root.
    ///
    /// IMPORTANT: Easy to mistake for [`GenesisInfo::get_state_circulating_supply`], that's being
    /// calculated differently.
    pub fn get_vm_circulating_supply<DB: Blockstore>(
        &self,
        height: ChainEpoch,
        db: &Arc<DB>,
        root: &Cid,
    ) -> Result<TokenAmount, anyhow::Error> {
        let detailed = self.get_vm_circulating_supply_detailed(height, db, root)?;

        Ok(detailed.fil_circulating)
    }

    /// Calculate total FIL circulating supply based on Genesis configuration and state of particular
    /// actors at a given height and state root.
    pub fn get_vm_circulating_supply_detailed<DB: Blockstore>(
        &self,
        height: ChainEpoch,
        db: &Arc<DB>,
        root: &Cid,
    ) -> anyhow::Result<CirculatingSupply> {
        let state_tree = StateTree::new_from_root(Arc::clone(db), root)?;

        let fil_vested = get_fil_vested(self, height);
        let fil_mined = get_fil_mined(&state_tree)?;
        let fil_burnt = get_fil_burnt(&state_tree)?;

        let network_version = self.chain_config.network_version(height);
        let fil_locked = get_fil_locked(&state_tree, network_version)?;
        let fil_reserve_disbursed = if height > self.chain_config.epoch(Height::Assembly) {
            get_fil_reserve_disbursed(&self.chain_config, height, &state_tree)?
        } else {
            TokenAmount::default()
        };
        let fil_circulating = TokenAmount::max(
            &fil_vested + &fil_mined + &fil_reserve_disbursed - &fil_burnt - &fil_locked,
            TokenAmount::default(),
        );
        Ok(CirculatingSupply {
            fil_vested,
            fil_mined,
            fil_burnt,
            fil_locked,
            fil_circulating,
            fil_reserve_disbursed,
        })
    }

    /// Calculate total FIL circulating supply based on state, traversing the state tree and
    /// checking Actor types. This can be a lengthy operation.
    ///
    /// IMPORTANT: Easy to mistake for [`GenesisInfo::get_vm_circulating_supply`], that's being
    /// calculated differently.
    pub fn get_state_circulating_supply<DB: Blockstore>(
        &self,
        height: ChainEpoch,
        db: &Arc<DB>,
        root: &Cid,
    ) -> Result<TokenAmount, anyhow::Error> {
        let mut circ = TokenAmount::default();
        let mut un_circ = TokenAmount::default();

        let state_tree = StateTree::new_from_root(Arc::clone(db), root)?;

        state_tree.for_each(|addr: Address, actor: &ActorState| {
            let actor_balance = TokenAmount::from(actor.balance.clone());
            if !actor_balance.is_zero() {
                match addr {
                    Address::INIT_ACTOR
                    | Address::REWARD_ACTOR
                    | Address::VERIFIED_REGISTRY_ACTOR
                    // The power actor itself should never receive funds
                    | Address::POWER_ACTOR
                    | Address::SYSTEM_ACTOR
                    | Address::CRON_ACTOR
                    | Address::BURNT_FUNDS_ACTOR
                    | Address::SAFT_ACTOR
                    | Address::RESERVE_ACTOR
                    | Address::ETHEREUM_ACCOUNT_MANAGER_ACTOR => {
                        un_circ += actor_balance;
                    }
                    Address::MARKET_ACTOR => {
                        let network_version = self.chain_config.network_version(height);
                        if network_version >= NetworkVersion::V23 {
                            circ += actor_balance;
                        } else {
                            let ms = market::State::load(db, actor.code, actor.state)?;
                            let locked_balance = ms.total_locked();
                            circ += actor_balance - &locked_balance;
                            un_circ += locked_balance;
                        }
                    }
                    _ if is_account_actor(&actor.code)
                    || is_paymentchannel_actor(&actor.code)
                    || is_ethaccount_actor(&actor.code)
                    || is_evm_actor(&actor.code)
                    || is_placeholder_actor(&actor.code) => {
                        circ += actor_balance;
                    },
                    _ if is_miner_actor(&actor.code) => {
                        let ms = miner::State::load(&db, actor.code, actor.state)?;

                        if let Ok(avail_balance) = ms.available_balance(actor.balance.atto()) {
                            circ += avail_balance.clone();
                            un_circ += actor_balance.clone() - &avail_balance;
                        } else {
                            // Assume any error is because the miner state is "broken" (lower actor balance than locked funds)
                            // In this case, the actor's entire balance is considered "uncirculating"
                            un_circ += actor_balance;
                        }
                    }
                    _ if is_multisig_actor(&actor.code) => {
                        let ms = multisig::State::load(&db, actor.code, actor.state)?;

                        let locked_balance = ms.locked_balance(height)?;
                        let avail_balance = actor_balance.clone() - &locked_balance;
                        circ += avail_balance.max(TokenAmount::zero());
                        un_circ += actor_balance.min(locked_balance);
                    }
                    _ => bail!("unexpected actor: {:?}", actor),
                }
            } else {
                // Do nothing for zero-balance actors
            }
            Ok(())
        })?;

        let total = circ.clone() + un_circ;
        if total != *TOTAL_FILECOIN {
            bail!(
                "total filecoin didn't add to expected amount: {} != {}",
                total,
                *TOTAL_FILECOIN
            );
        }

        Ok(circ)
    }
}

/// Vesting schedule info. These states are lazily filled, to avoid doing until
/// needed to calculate circulating supply.
#[derive(Default, Clone)]
struct GenesisInfoVesting {
    genesis: Vec<(ChainEpoch, TokenAmount)>,
    ignition: Vec<(ChainEpoch, ChainEpoch, TokenAmount)>,
    calico: Vec<(ChainEpoch, ChainEpoch, TokenAmount)>,
}

impl GenesisInfoVesting {
    fn new(liftoff_height: i64) -> Self {
        Self {
            genesis: setup_genesis_vesting_schedule(),
            ignition: setup_ignition_vesting_schedule(liftoff_height),
            calico: setup_calico_vesting_schedule(liftoff_height),
        }
    }
}

fn get_actor_state<DB: Blockstore>(
    state_tree: &StateTree<DB>,
    addr: &Address,
) -> Result<ActorState, anyhow::Error> {
    state_tree
        .get_actor(addr)?
        .with_context(|| format!("Failed to get Actor for address {addr}"))
}

fn get_fil_vested(genesis_info: &GenesisInfo, height: ChainEpoch) -> TokenAmount {
    let mut return_value = TokenAmount::default();

    let pre_ignition = &genesis_info.vesting.genesis;
    let post_ignition = &genesis_info.vesting.ignition;
    let calico_vesting = &genesis_info.vesting.calico;

    if height <= genesis_info.chain_config.epoch(Height::Ignition) {
        for (unlock_duration, initial_balance) in pre_ignition {
            return_value +=
                initial_balance - v0_amount_locked(*unlock_duration, initial_balance, height);
        }
    } else if height <= genesis_info.chain_config.epoch(Height::Calico) {
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

    if height <= genesis_info.chain_config.epoch(Height::Assembly) {
        return_value += &genesis_info.genesis_pledge + &genesis_info.genesis_market_funds;
    }

    return_value
}

fn get_fil_mined<DB: Blockstore>(state_tree: &StateTree<DB>) -> Result<TokenAmount, anyhow::Error> {
    let state: reward::State = state_tree.get_actor_state()?;
    Ok(state.into_total_storage_power_reward())
}

fn get_fil_market_locked<DB: Blockstore>(
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, anyhow::Error> {
    let actor = state_tree
        .get_actor(&Address::MARKET_ACTOR)?
        .ok_or_else(|| Error::state("Market actor address could not be resolved"))?;
    let state = market::State::load(state_tree.store(), actor.code, actor.state)?;

    Ok(state.total_locked())
}

fn get_fil_power_locked<DB: Blockstore>(
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, anyhow::Error> {
    let actor = state_tree
        .get_actor(&Address::POWER_ACTOR)?
        .ok_or_else(|| Error::state("Power actor address could not be resolved"))?;
    let state = power::State::load(state_tree.store(), actor.code, actor.state)?;
    Ok(state.into_total_locked())
}

fn get_fil_reserve_disbursed<DB: Blockstore>(
    chain_config: &ChainConfig,
    height: ChainEpoch,
    state_tree: &StateTree<DB>,
) -> Result<TokenAmount, anyhow::Error> {
    // FIP-0100 introduced a different hard-coded reserved amount for testnets.
    // See <https://github.com/filecoin-project/FIPs/blob/master/FIPS/fip-0100.md#special-handling-for-calibration-network>
    // for details.
    let fil_reserved = chain_config.initial_fil_reserved_at_height(height);
    let reserve_actor = get_actor_state(state_tree, &Address::RESERVE_ACTOR)?;

    // If money enters the reserve actor, this could lead to a negative term
    Ok(fil_reserved - TokenAmount::from(&reserve_actor.balance))
}

fn get_fil_locked<DB: Blockstore>(
    state_tree: &StateTree<DB>,
    network_version: NetworkVersion,
) -> Result<TokenAmount, anyhow::Error> {
    let total = if network_version >= NetworkVersion::V23 {
        get_fil_power_locked(state_tree)?
    } else {
        get_fil_market_locked(state_tree)? + get_fil_power_locked(state_tree)?
    };

    Ok(total)
}

fn get_fil_burnt<DB: Blockstore>(state_tree: &StateTree<DB>) -> Result<TokenAmount, anyhow::Error> {
    let burnt_actor = get_actor_state(state_tree, &Address::BURNT_FUNDS_ACTOR)?;

    Ok(TokenAmount::from(&burnt_actor.balance))
}

fn setup_genesis_vesting_schedule() -> Vec<(ChainEpoch, TokenAmount)> {
    PRE_CALICO_VESTING
        .into_iter()
        .map(|(unlock_duration, initial_balance)| {
            (unlock_duration, TokenAmount::from_atto(initial_balance))
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
                TokenAmount::from_whole(initial_balance),
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
                TokenAmount::from_whole(initial_balance),
            )
        })
        .collect()
}

// This exact code (bugs and all) has to be used. The results are locked into
// the blockchain.
/// Returns amount locked in multisig contract
fn v0_amount_locked(
    unlock_duration: ChainEpoch,
    initial_balance: &TokenAmount,
    elapsed_epoch: ChainEpoch,
) -> TokenAmount {
    if elapsed_epoch >= unlock_duration {
        return TokenAmount::zero();
    }
    if elapsed_epoch < 0 {
        return initial_balance.clone();
    }
    // Division truncation is broken here: https://github.com/filecoin-project/specs-actors/issues/1131
    let unit_locked: TokenAmount = initial_balance.div_floor(unlock_duration);
    unit_locked * (unlock_duration - elapsed_epoch)
}
