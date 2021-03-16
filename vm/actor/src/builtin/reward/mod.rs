// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub(crate) mod expneg;
mod logic;
mod state;
mod types;

pub use self::logic::*;
pub use self::state::{Reward, State, VestingFunction};
pub use self::types::*;
use crate::network::EXPECTED_LEADERS_PER_EPOCH;
use crate::{
    check_empty_params, miner, BURNT_FUNDS_ACTOR_ADDR, STORAGE_POWER_ACTOR_ADDR, SYSTEM_ACTOR_ADDR,
};
use fil_types::StoragePower;
use ipld_blockstore::BlockStore;
use log::{error, warn};
use num_bigint::Sign;
use num_bigint::{bigint_ser::BigIntDe, Integer};
use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, Signed};
use runtime::{ActorCode, Runtime};
use vm::{
    actor_error, ActorError, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR,
    METHOD_SEND,
};

// * Updated to specs-actors commit: 999e57a151cc7ada020ca2844b651499ab8c0dec (v3.0.1)

/// PenaltyMultiplier is the factor miner penaltys are scaled up by
pub const PENALTY_MULTIPLIER: u64 = 3;

/// Reward actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    AwardBlockReward = 2,
    ThisEpochReward = 3,
    UpdateNetworkKPI = 4,
}

/// Reward Actor
pub struct Actor;
impl Actor {
    /// Constructor for Reward actor
    fn constructor<BS, RT>(
        rt: &mut RT,
        curr_realized_power: Option<StoragePower>,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*SYSTEM_ACTOR_ADDR))?;

        if let Some(power) = curr_realized_power {
            rt.create(&State::new(power))?;
            Ok(())
        } else {
            Err(actor_error!(
                ErrIllegalArgument,
                "argument should not be nil"
            ))
        }
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
        let prior_balance = rt.current_balance()?;
        if params.penalty.sign() == Sign::Minus {
            return Err(actor_error!(
                ErrIllegalArgument,
                "negative penalty {}",
                params.penalty
            ));
        }
        if params.gas_reward.sign() == Sign::Minus {
            return Err(actor_error!(
                ErrIllegalArgument,
                "negative gas reward {}",
                params.gas_reward
            ));
        }
        if prior_balance < params.gas_reward {
            return Err(actor_error!(
                ErrIllegalState,
                "actor current balance {} insufficient to pay gas reward {}",
                prior_balance,
                params.gas_reward
            ));
        }
        if params.win_count <= 0 {
            return Err(actor_error!(
                ErrIllegalArgument,
                "invalid win count {}",
                params.win_count
            ));
        }

        let miner_addr = rt
            .resolve_address(&params.miner)?
            .ok_or_else(|| actor_error!(ErrNotFound, "failed to resolve given owner address"))?;

        let penalty: TokenAmount = &params.penalty * PENALTY_MULTIPLIER;

        let total_reward = rt.transaction(|st: &mut State, rt| {
            let mut block_reward: TokenAmount = (&st.this_epoch_reward * params.win_count)
                .div_floor(&TokenAmount::from(EXPECTED_LEADERS_PER_EPOCH));
            let mut total_reward = &params.gas_reward + &block_reward;
            let curr_balance = rt.current_balance()?;
            if total_reward > curr_balance {
                warn!(
                    "reward actor balance {} below totalReward expected {},\
                    paying out rest of balance",
                    curr_balance, total_reward
                );
                total_reward = curr_balance;
                block_reward = &total_reward - &params.gas_reward;
                if block_reward.is_negative() {
                    return Err(actor_error!(
                        ErrIllegalState,
                        "programming error, block reward {} below zero",
                        block_reward
                    ));
                }
            }
            st.total_storage_power_reward += block_reward;
            Ok(total_reward)
        })?;

        // * Go implementation added this and removed capping it -- this could potentially panic
        // * as they treat panics as an exit code. Revisit this.
        if total_reward > prior_balance {
            return Err(actor_error!(
                ErrIllegalState,
                "reward {} exceeds balance {}",
                total_reward,
                prior_balance
            ));
        }

        // if this fails, we can assume the miner is responsible and avoid failing here.
        let reward_params = miner::ApplyRewardParams {
            reward: total_reward.clone(),
            penalty,
        };
        let res = rt.send(
            miner_addr,
            miner::Method::ApplyRewards as u64,
            Serialized::serialize(&reward_params)?,
            total_reward.clone(),
        );
        if let Err(e) = res {
            error!(
                "failed to send ApplyRewards call to the miner actor with funds {}, code: {:?}",
                total_reward,
                e.exit_code()
            );
            let res = rt.send(
                *BURNT_FUNDS_ACTOR_ADDR,
                METHOD_SEND,
                Serialized::default(),
                total_reward,
            );
            if let Err(e) = res {
                error!(
                    "failed to send unsent reward to the burnt funds actor, code: {:?}",
                    e.exit_code()
                );
            }
        }

        Ok(())
    }

    /// The award value used for the current epoch, updated at the end of an epoch
    /// through cron tick.  In the case previous epochs were null blocks this
    /// is the reward value as calculated at the last non-null epoch.
    fn this_epoch_reward<BS, RT>(rt: &mut RT) -> Result<ThisEpochRewardReturn, ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_accept_any()?;
        let st: State = rt.state()?;
        Ok(ThisEpochRewardReturn {
            this_epoch_baseline_power: st.this_epoch_baseline_power,
            this_epoch_reward_smoothed: st.this_epoch_reward_smoothed,
        })
    }

    /// Called at the end of each epoch by the power actor (in turn by its cron hook).
    /// This is only invoked for non-empty tipsets, but catches up any number of null
    /// epochs to compute the next epoch reward.
    fn update_network_kpi<BS, RT>(
        rt: &mut RT,
        curr_realized_power: Option<StoragePower>,
    ) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        rt.validate_immediate_caller_is(std::iter::once(&*STORAGE_POWER_ACTOR_ADDR))?;
        let curr_realized_power = curr_realized_power
            .ok_or_else(|| actor_error!(ErrIllegalArgument, "argument cannot be None"))?;

        rt.transaction(|st: &mut State, rt| {
            let prev = st.epoch;
            // if there were null runs catch up the computation until
            // st.Epoch == rt.CurrEpoch()
            while st.epoch < rt.curr_epoch() {
                // Update to next epoch to process null rounds
                st.update_to_next_epoch(&curr_realized_power);
            }

            st.update_to_next_epoch_with_reward(&curr_realized_power);
            st.update_smoothed_estimates(st.epoch - prev);
            Ok(())
        })?;
        Ok(())
    }
}

impl ActorCode for Actor {
    fn invoke_method<BS, RT>(
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
                let param: Option<BigIntDe> = rt.deserialize_params(params)?;
                Self::constructor(rt, param.map(|v| v.0))?;
                Ok(Serialized::default())
            }
            Some(Method::AwardBlockReward) => {
                Self::award_block_reward(rt, rt.deserialize_params(params)?)?;
                Ok(Serialized::default())
            }
            Some(Method::ThisEpochReward) => {
                check_empty_params(params)?;
                let res = Self::this_epoch_reward(rt)?;
                Ok(Serialized::serialize(&res)?)
            }
            Some(Method::UpdateNetworkKPI) => {
                let param: Option<BigIntDe> = rt.deserialize_params(params)?;
                Self::update_network_kpi(rt, param.map(|v| v.0))?;
                Ok(Serialized::default())
            }
            None => Err(actor_error!(SysErrInvalidMethod, "Invalid method")),
        }
    }
}
