// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::state_computation::apply_block_messages_blocking;
use super::utils::structured;
use super::*;
use crate::blocks::CachingBlockHeader;
use crate::interpreter::{CalledAt, VMTrace};
use crate::rpc::state::{BlockReward, ExecutionTrace, RewardAmounts};
use crate::shim::actors::reward::{RewardDistributionError, award_distribution};
use anyhow::ensure;
use fil_actor_reward_state::v19::DENOM;
use num_traits::Zero as _;

impl StateManager {
    /// Returns the block rewards allocated while executing `tipset`.
    ///
    /// Results are cached by tipset key, and concurrent calls for the same tipset share one
    /// traced execution. A tipset at epoch 0 is not executed, so it has no block rewards, as in
    /// Lotus:
    /// <https://github.com/filecoin-project/lotus/blob/v1.37.0-rc2/chain/consensus/compute_state.go#L357-L363>
    ///
    /// # Errors
    /// Fails with [`RewardDistributionError::RewardActorBeforeV19`] for a tipset executed before
    /// network version 29, and when `tipset` cannot be executed.
    pub async fn reward_distribution(
        &self,
        tipset: &Tipset,
    ) -> anyhow::Result<Arc<RewardDistribution>> {
        ensure!(
            self.get_network_version(tipset.epoch()) >= NetworkVersion::V29,
            RewardDistributionError::RewardActorBeforeV19
        );
        self.reward_distribution_cache
            .get_or_insert_async(
                tipset.key(),
                self.reward_distribution_inner(tipset.shallow_clone()),
            )
            .await
    }

    async fn reward_distribution_inner(
        &self,
        tipset: Tipset,
    ) -> anyhow::Result<Arc<RewardDistribution>> {
        let permit = self.replay_permit().await;
        let this = self.shallow_clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            this.reward_distribution_inner_blocking(tipset)
        })
        .await
        .context("tokio join error")?
    }

    fn reward_distribution_inner_blocking(
        &self,
        tipset: Tipset,
    ) -> anyhow::Result<Arc<RewardDistribution>> {
        let mut blocks = Vec::with_capacity(tipset.len());

        // Reward messages are applied once per block, in block order.
        let callback = |ctx: MessageCallbackCtx<'_>| {
            if matches!(ctx.at, CalledAt::Reward) {
                let header = tipset
                    .block_headers()
                    .get(blocks.len())
                    .context("more reward messages than blocks")?;
                blocks.push(block_reward(header, &ctx)?);
            }
            Ok(())
        };

        apply_block_messages_blocking(
            self.chain_index().shallow_clone(),
            self.chain_config().shallow_clone(),
            self.beacon_schedule().shallow_clone(),
            &self.engine,
            tipset.shallow_clone(),
            Some(callback),
            VMTrace::Traced,
        )?;

        Ok(Arc::new(RewardDistribution {
            tipset_key: tipset.key().clone(),
            height: tipset.epoch(),
            denom: DENOM,
            totals: total_amounts(&blocks),
            blocks,
        }))
    }
}

/// Reconstructs the reward of the block mined by `header` from its reward message.
fn block_reward(
    header: &CachingBlockHeader,
    ctx: &MessageCallbackCtx<'_>,
) -> anyhow::Result<BlockReward> {
    let params: AwardBlockRewardParams = ctx.message.message().params().deserialize()?;
    let miner = Address::from(params.miner);
    ensure!(
        miner == header.miner_address,
        "reward message pays {miner} for a block mined by {}",
        header.miner_address
    );
    let trace = structured::parse_events(ctx.apply_ret.exec_trace())?
        .context("reward message has no execution trace")?;

    // The reward actor as the message found it and as the message left it.
    let actor_before = trace
        .invoked_actor
        .context("reward message invoked no actor")?
        .state;
    let actor_after = ctx
        .state
        .get_actor(&Address::REWARD_ACTOR)?
        .context("reward actor not found")?;
    let before = reward::State::load_from_blockstore(&ctx.state, &actor_before)?;
    let after = reward::State::load_from_blockstore(&ctx.state, &actor_after)?;

    let (streams, burn_weight, mut amounts) =
        award_distribution(&before, &after, &ctx.state, header.epoch)?;
    amounts.message_reward = params.gas_reward.into();
    (amounts.miner_paid, amounts.burn_paid) = direct_payments(trace.subcalls, miner);

    Ok(BlockReward {
        block: *header.cid(),
        miner,
        win_count: params.win_count,
        amounts,
        burn_weight,
        streams,
    })
}

/// Sums the amounts of `blocks` field by field.
fn total_amounts(blocks: &[BlockReward]) -> RewardAmounts {
    let mut totals = RewardAmounts::default();
    for block in blocks {
        let RewardAmounts {
            minted_reward,
            miner_reward,
            message_reward,
            explicit_reward,
            burn_allocation,
            miner_paid,
            burn_paid,
        } = block.amounts.clone();
        totals.minted_reward += minted_reward;
        totals.miner_reward += miner_reward;
        totals.message_reward += message_reward;
        totals.explicit_reward += explicit_reward;
        totals.burn_allocation += burn_allocation;
        totals.miner_paid += miner_paid;
        totals.burn_paid += burn_paid;
    }
    totals
}

/// Sums what the successful calls made by the reward actor `(f02)` itself transferred to `miner` and
/// to the burnt funds actor.
///
/// Transfers nested inside those calls are not counted, as in Lotus:
/// <https://github.com/filecoin-project/lotus/blob/v1.37.0-rc2/chain/stmgr/rewards.go#L155-L167>
fn direct_payments(subcalls: Vec<ExecutionTrace>, miner: Address) -> (TokenAmount, TokenAmount) {
    let mut miner_paid = TokenAmount::zero();
    let mut burn_paid = TokenAmount::zero();
    for call in subcalls {
        if !call.msg_rct.exit_code.is_success() {
            continue;
        }
        if call.msg.to == miner {
            miner_paid += call.msg.value;
        } else if call.msg.to == Address::BURNT_FUNDS_ACTOR {
            burn_paid += call.msg.value;
        }
    }
    (miner_paid, burn_paid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::state::{MessageTrace, ReturnTrace};
    use crate::shim::error::ExitCode;

    /// A call the reward actor made while it ran the award, sending `value` to `to`.
    fn transfer(to: Address, value: i64, exit_code: u32) -> ExecutionTrace {
        ExecutionTrace {
            msg: MessageTrace {
                from: Address::REWARD_ACTOR,
                to,
                value: TokenAmount::from_atto(value),
                method: 0,
                params: Default::default(),
                params_codec: 0,
                gas_limit: None,
                read_only: None,
            },
            msg_rct: ReturnTrace {
                exit_code: ExitCode::from(exit_code),
                r#return: Default::default(),
                return_codec: 0,
            },
            invoked_actor: None,
            gas_charges: vec![],
            subcalls: vec![],
            logs: vec![],
            ipld_ops: vec![],
        }
    }

    /// An award pays by calling the miner and the burn address itself. `direct_payments` adds up
    /// what those calls sent when they succeeded. What is sent further inside them is not a
    /// payment of the award.
    #[test]
    fn direct_payments_sum_successful_transfers_to_the_miner_and_to_burn() {
        const OK: u32 = 0;
        const ILLEGAL_STATE: u32 = 20;
        let miner = Address::new_id(1000);
        // The miner burns a penalty inside its own call, which is not a payment of the award.
        let mut miner_payment = transfer(miner, 107, OK);
        miner_payment.subcalls = vec![transfer(Address::BURNT_FUNDS_ACTOR, 13, OK)];
        let subcalls = vec![
            miner_payment,
            transfer(miner, 41, ILLEGAL_STATE),
            transfer(Address::BURNT_FUNDS_ACTOR, 2, OK),
            transfer(Address::BURNT_FUNDS_ACTOR, 20, OK),
            transfer(Address::BURNT_FUNDS_ACTOR, 99, ILLEGAL_STATE),
            transfer(Address::new_id(1001), 5, OK),
        ];

        let (miner_paid, burn_paid) = direct_payments(subcalls, miner);

        assert_eq!(miner_paid, TokenAmount::from_atto(107)); // the failed 41 is left out
        // 2 + 20; the nested 13 and the failed 99 are left out
        assert_eq!(burn_paid, TokenAmount::from_atto(22));
    }
}
