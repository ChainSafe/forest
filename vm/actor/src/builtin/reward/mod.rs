// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;
mod types;

pub use self::state::{Reward, State, VestingFunction};
pub use self::types::*;
use crate::network::EXPECTED_LEADERS_PER_EPOCH;
use crate::{
    check_empty_params, miner, BURNT_FUNDS_ACTOR_ADDR, STORAGE_POWER_ACTOR_ADDR, SYSTEM_ACTOR_ADDR,
};
use clock::ChainEpoch;
use fil_types::StoragePower;
use ipld_blockstore::BlockStore;
use num_bigint::biguint_ser::{BigUintDe, BigUintSer};
use num_bigint::BigUint;
use num_derive::FromPrimitive;
use num_traits::{CheckedSub, FromPrimitive};
use runtime::{ActorCode, Runtime};
use vm::{
    ActorError, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR, METHOD_SEND,
};

// * Updated to specs-actors commit: 52599b21919df07f44d7e61cc028e265ec18f700

/// Reward actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    AwardBlockReward = 2,
    LastPerEpochReward = 3,
    UpdateNetworkKPI = 4,
}

/// Reward Actor
pub struct Actor;
impl Actor {
    /// Constructor for Reward actor
    fn constructor<BS, RT>(rt: &mut RT) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*SYSTEM_ACTOR_ADDR))?;

        // TODO revisit based on issue: https://github.com/filecoin-project/specs-actors/issues/317

        rt.create(&State::new())?;
        Ok(())
    }

    /// Awards a reward to a block producer.
    /// This method is called only by the system actor, implicitly, as the last message in the evaluation of a block.
    /// The system actor thus computes the parameters and attached value.
    ///
    /// The reward includes two components:
    /// - the epoch block reward, computed and paid from the reward actor's balance,
    /// - the block gas reward, expected to be transferred to the reward actor with this invocation.
    ///
    /// The reward is reduced before the residual is credited to the block producer, by:
    /// - a penalty amount, provided as a parameter, which is burnt,
    fn award_block_reward<BS, RT>(
        rt: &mut RT,
        params: AwardBlockRewardParams,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*SYSTEM_ACTOR_ADDR))?;
        let balance = rt.current_balance()?;
        assert!(
            balance >= params.gas_reward,
            "actor current balance {} insufficient to pay gas reward {}",
            balance,
            params.gas_reward
        );

        assert!(
            params.ticket_count > 0,
            "cannot give block reward for zero tickets"
        );

        let miner_addr = rt
            .resolve_address(&params.miner)
            // TODO revisit later if all address resolutions end up being the same exit code
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e.msg().to_string()))?;

        let prior_balance = rt.current_balance()?;

        let state: State = rt.state()?;
        let block_reward = state.last_per_epoch_reward / EXPECTED_LEADERS_PER_EPOCH;
        let total_reward = block_reward + params.gas_reward;

        // Cap the penalty at the total reward value.
        let penalty = std::cmp::min(&params.penalty, &total_reward);

        // Reduce the payable reward by the penalty.
        let reward_payable = total_reward.clone() - penalty;

        assert!(
            reward_payable <= prior_balance - penalty,
            "Total reward exceeds balance of actor"
        );

        rt.send(
            &miner_addr,
            miner::Method::AddLockedFund as u64,
            &Serialized::serialize(&BigUintSer(&reward_payable)).unwrap(),
            &reward_payable,
        )?;

        // Burn the penalty
        rt.send(
            &*BURNT_FUNDS_ACTOR_ADDR,
            METHOD_SEND,
            &Serialized::default(),
            &penalty,
        )?;

        Ok(())
    }

    fn last_per_epoch_reward<BS, RT>(rt: &RT) -> Result<TokenAmount, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any();
        let st: State = rt.state()?;
        Ok(st.last_per_epoch_reward)
    }

    /// Withdraw available funds from reward map
    fn compute_per_epoch_reward(st: &mut State, _ticket_count: u64) -> TokenAmount {
        // TODO update when finished in specs
        let new_simple_supply = minting_function(
            &SIMPLE_TOTAL,
            &(BigUint::from(st.reward_epochs_paid) << MINTING_INPUT_FIXED_POINT),
        );
        let new_baseline_supply = minting_function(&*BASELINE_TOTAL, &st.effective_network_time);

        let new_simple_minted = new_simple_supply
            .checked_sub(&st.simple_supply)
            .unwrap_or_default();
        let new_baseline_minted = new_baseline_supply
            .checked_sub(&st.baseline_supply)
            .unwrap_or_default();

        st.simple_supply = new_simple_supply;
        st.baseline_supply = new_baseline_supply;

        let per_epoch_reward = new_simple_minted + new_baseline_minted;
        st.last_per_epoch_reward = per_epoch_reward.clone();
        per_epoch_reward
    }

    fn new_baseline_power(_st: &State, _reward_epochs_paid: ChainEpoch) -> StoragePower {
        // TODO: this is not the final baseline function or value, PARAM_FINISH
        BigUint::from(BASELINE_POWER)
    }

    // Called at the end of each epoch by the power actor (in turn by its cron hook).
    // This is only invoked for non-empty tipsets. The impact of this is that block rewards are paid out over
    // a schedule defined by non-empty tipsets, not by elapsed time/epochs.
    // This is not necessarily what we want, and may change.
    fn update_network_kpi<BS, RT>(
        rt: &mut RT,
        curr_realized_power: StoragePower,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*STORAGE_POWER_ACTOR_ADDR))?;

        rt.transaction(|st: &mut State, _| {
            // By the time this is called, the rewards for this epoch have been paid to miners.
            st.reward_epochs_paid += 1;
            st.realized_power = curr_realized_power;

            st.baseline_power = Self::new_baseline_power(st, st.reward_epochs_paid);
            st.cumsum_baseline += &st.baseline_power;

            // Cap realized power in computing CumsumRealized so that progress is only relative to the current epoch.
            let capped_realized_power = std::cmp::min(&st.baseline_power, &st.realized_power);
            st.cumsum_realized += capped_realized_power;
            st.effective_network_time =
                st.get_effective_network_time(&st.cumsum_baseline, &st.cumsum_realized);
            Self::compute_per_epoch_reward(st, 1);
        })?;
        Ok(())
    }
}

impl ActorCode for Actor {
    fn invoke_method<BS, RT>(
        &self,
        rt: &mut RT,
        method: MethodNum,
        params: &Serialized,
    ) -> Result<Serialized, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        match FromPrimitive::from_u64(method) {
            Some(Method::Constructor) => {
                check_empty_params(params)?;
                Self::constructor(rt)?;
                Ok(Serialized::default())
            }
            Some(Method::AwardBlockReward) => {
                Self::award_block_reward(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            Some(Method::LastPerEpochReward) => {
                let res = Self::last_per_epoch_reward(rt)?;
                Ok(Serialized::serialize(BigUintSer(&res))?)
            }
            Some(Method::UpdateNetworkKPI) => {
                let BigUintDe(param) = params.deserialize()?;
                Self::update_network_kpi(rt, param)?;
                Ok(Serialized::default())
            }
            _ => Err(rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method")),
        }
    }
}
