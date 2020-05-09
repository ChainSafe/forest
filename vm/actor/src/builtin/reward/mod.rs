// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod state;
mod types;

pub use self::state::{Reward, State, VestingFunction};
pub use self::types::*;
use crate::{
    check_empty_params, request_miner_control_addrs, Multimap, BURNT_FUNDS_ACTOR_ADDR,
    SYSTEM_ACTOR_ADDR,
};
use address::Address;
use ipld_blockstore::BlockStore;
use num_derive::FromPrimitive;
use num_traits::{FromPrimitive, Zero};
use runtime::{ActorCode, Runtime};
use vm::{
    ActorError, ExitCode, MethodNum, Serialized, TokenAmount, METHOD_CONSTRUCTOR, METHOD_SEND,
};

/// Reward actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    AwardBlockReward = 2,
    WithdrawReward = 3,
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

        let empty_root = Multimap::new(rt.store()).root().map_err(|e| {
            ActorError::new(
                ExitCode::ErrIllegalState,
                format!("failed to construct state: {}", e),
            )
        })?;

        rt.create(&State::new(empty_root))?;
        Ok(())
    }

    /// Mints a reward and puts into state reward map
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
        if balance < params.gas_reward {
            return Err(ActorError::new(
                ExitCode::ErrInsufficientFunds,
                format!(
                    "actor current balance {} insufficient to pay gas reward {}",
                    balance, params.gas_reward
                ),
            ));
        }

        if params.ticket_count == 0 {
            return Err(ActorError::new(
                ExitCode::ErrIllegalArgument,
                "cannot give block reward for zero tickets".to_owned(),
            ));
        }

        let miner = rt.resolve_address(&params.miner)?;

        let prior_bal = rt.current_balance()?;

        let cur_epoch = rt.curr_epoch();
        let penalty: TokenAmount = rt
            .transaction::<_, Result<_, String>, _>(|st: &mut State, rt| {
                let block_rew = Self::compute_block_reward(
                    st,
                    &prior_bal - &params.gas_reward,
                    params.ticket_count,
                );
                let total_reward = block_rew + &params.gas_reward;

                // Cap the penalty at the total reward value.
                let penalty = std::cmp::min(params.penalty, total_reward.clone());
                // Reduce the payable reward by the penalty.
                let rew_payable = total_reward - &penalty;
                if (&rew_payable + &penalty) > prior_bal {
                    return Err(format!(
                        "reward payable {} + penalty {} exceeds balance {}",
                        rew_payable, penalty, prior_bal
                    ));
                }

                // Record new reward into reward map.
                if rew_payable > TokenAmount::zero() {
                    st.add_reward(
                        rt.store(),
                        &miner,
                        Reward {
                            start_epoch: cur_epoch,
                            end_epoch: cur_epoch + REWARD_VESTING_PERIOD,
                            value: rew_payable,
                            amount_withdrawn: TokenAmount::zero(),
                            vesting_function: REWARD_VESTING_FUNCTION,
                        },
                    )?;
                }
                //
                Ok(penalty)
            })?
            .map_err(|e| ActorError::new(ExitCode::ErrIllegalState, e))?;

        // Burn the penalty
        rt.send(
            &*BURNT_FUNDS_ACTOR_ADDR,
            METHOD_SEND,
            &Serialized::default(),
            &penalty,
        )?;

        Ok(())
    }

    /// Withdraw available funds from reward map
    fn withdraw_reward<BS, RT>(rt: &mut RT, miner_in: Address) -> Result<(), ActorError>
    where
        BS: BlockStore,
        RT: Runtime<BS>,
    {
        let maddr = rt.resolve_address(&miner_in)?;

        let (owner, worker) = request_miner_control_addrs(rt, &maddr)?;

        rt.validate_immediate_caller_is([owner, worker].iter())?;

        let cur_epoch = rt.curr_epoch();
        let withdrawable_reward =
            rt.transaction::<_, Result<_, ActorError>, _>(|st: &mut State, rt| {
                let withdrawn = st
                    .withdraw_reward(rt.store(), &maddr, cur_epoch)
                    .map_err(|e| {
                        ActorError::new(
                            ExitCode::ErrIllegalState,
                            format!("failed to withdraw record: {}", e),
                        )
                    })?;
                Ok(withdrawn)
            })??;

        rt.send(
            &owner,
            METHOD_SEND,
            &Serialized::default(),
            &withdrawable_reward,
        )?;
        Ok(())
    }

    /// Withdraw available funds from reward map
    fn compute_block_reward(st: &State, balance: TokenAmount, ticket_count: u64) -> TokenAmount {
        let treasury = balance - &st.reward_total;
        let target_rew = BLOCK_REWARD_TARGET.clone() * ticket_count;

        std::cmp::min(target_rew, treasury)
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
            Some(Method::WithdrawReward) => {
                Self::withdraw_reward(rt, params.deserialize()?)?;
                Ok(Serialized::default())
            }
            _ => Err(rt.abort(ExitCode::SysErrInvalidMethod, "Invalid method")),
        }
    }
}
