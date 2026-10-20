// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::State;
use crate::rpc::state::{ExplicitRewardDistribution, RecipientReward, RewardAmounts, StreamReward};
use crate::shim::clock::ChainEpoch;
use crate::shim::econ::TokenAmount;
use anyhow::{Context as _, bail, ensure};
use fil_actor_reward_state::v19::{DENOM, Ledger, compute_weight};
use fvm_ipld_blockstore::Blockstore;
use num_traits::Zero as _;

#[cfg(test)]
mod tests;

/// Why a block reward cannot be reconstructed.
#[derive(Debug, thiserror::Error)]
pub enum RewardDistributionError {
    /// State reward distribution is only applicable after the actor version 19
    #[error("StateRewardDistribution requires reward actor v19 (network version 29)")]
    RewardActorBeforeV19,
    /// The stream data failed the structure and accounting checks the reward actor runs before
    /// every award.
    #[error("reward actor rejects the stream data: {0:#}")]
    InvalidStreamData(#[source] anyhow::Error),
    #[error("reward allocation does not match committed explicit and burn minting counters")]
    AllocationMismatch,
    #[error("stream weights exceed the denominator")]
    WeightsExceedDenominator,
}

/// Reconstructs one award from the reward actor's state before it and after it.
///
/// Returns the reward of every stream, the burn weight and the minted amounts. The message
/// reward and the paid amounts are left at zero.
///
/// Taken from here:
/// <https://github.com/filecoin-project/lotus/blob/v1.37.0-rc2/chain/stmgr/rewards.go#L52-L107>
///
/// # Errors
/// Fails before actors v19, when the streams block of `after` is missing from `store` or fails
/// the reward actor's checks, when the reconstruction disagrees with the explicit and burn
/// counters the award committed, and when the stream weights exceed the denominator.
///
/// # Note:
/// This is not a direct translation from lotus in terms of the code structure so change it carefully.
pub fn award_distribution(
    before: &State,
    after: &State,
    store: &impl Blockstore,
    epoch: ChainEpoch,
) -> anyhow::Result<(Vec<StreamReward>, u64, RewardAmounts)> {
    match after {
        State::V19(after) => {
            let State::V19(before) = before else {
                bail!(RewardDistributionError::RewardActorBeforeV19)
            };
            award_distribution_v19(before, after, store, epoch)
        }
        State::V8(_)
        | State::V9(_)
        | State::V10(_)
        | State::V11(_)
        | State::V12(_)
        | State::V13(_)
        | State::V14(_)
        | State::V15(_)
        | State::V16(_)
        | State::V17(_)
        | State::V18(_) => bail!(RewardDistributionError::RewardActorBeforeV19),
    }
}

fn award_distribution_v19(
    before: &fil_actor_reward_state::v19::State,
    after: &fil_actor_reward_state::v19::State,
    store: &impl Blockstore,
    epoch: ChainEpoch,
) -> anyhow::Result<(Vec<StreamReward>, u64, RewardAmounts)> {
    let streams_block = store
        .get(&after.streams_root)?
        .with_context(|| format!("reward streams block {} not found", after.streams_root))?;
    // What every award runs on the streams block: decoding, then the structure and accounting
    // checks, which pair every explicit stream with one accrual row.
    let ledger = Ledger::decode_for_award(&streams_block, &after.accrued)
        .map_err(RewardDistributionError::InvalidStreamData)?;

    let minted_reward = TokenAmount::from(&after.total_minted_reward)
        - TokenAmount::from(&before.total_minted_reward);
    let explicit_minted = TokenAmount::from(&after.total_explicit_minted)
        - TokenAmount::from(&before.total_explicit_minted);
    let burn_minted =
        TokenAmount::from(&after.total_burn_minted) - TokenAmount::from(&before.total_burn_minted);

    let mut miner_reward = TokenAmount::zero();
    let mut explicit_reward = TokenAmount::zero();
    let mut streams = Vec::with_capacity(ledger.streams().streams.len());

    for stream in &ledger.streams().streams {
        let evaluated_weight = compute_weight(&stream.weight, epoch);
        let portion = (&minted_reward * evaluated_weight).div_floor(DENOM);
        let distribution = match &stream.distribution {
            None => {
                miner_reward += portion.clone();
                None
            }
            Some(distribution) => {
                let share_total = distribution.share_total();
                let accrual = (&portion * share_total).div_floor(DENOM);
                let pool: TokenAmount = after
                    .accrued
                    .iter()
                    .find(|accrual| accrual.id == stream.id)
                    .map(|accrual| (&accrual.amount).into())
                    .expect("accounting checks: every explicit stream has an accrual row");
                // A fold in this award empties the pool before the accrual, so the pool the
                // accrual was added to is derived from the pool after it, never read from the
                // earlier state.
                let pool_before = &pool - &accrual;

                let mut rounding_adjustment = accrual.clone();
                let mut recipients = Vec::with_capacity(distribution.shares.len());
                for row in &distribution.shares {
                    // The difference of two floored cumulative entitlements, so flooring dust
                    // of earlier awards can become earned here.
                    let earned_amount = (&pool * row.share).div_floor(share_total)
                        - (&pool_before * row.share).div_floor(share_total);
                    rounding_adjustment -= earned_amount.clone();
                    recipients.push(RecipientReward {
                        recipient: row.recipient.into(),
                        share: row.share,
                        earned_amount,
                    });
                }
                let burn_amount = &portion - &accrual;
                explicit_reward += accrual;
                Some(ExplicitRewardDistribution {
                    writer: distribution.writer.into(),
                    recipients,
                    burn_share: DENOM - share_total,
                    burn_amount,
                    rounding_adjustment,
                })
            }
        };
        streams.push(StreamReward {
            id: stream.id,
            weight: evaluated_weight,
            amount: portion,
            distribution,
        });
    }

    let burn_allocation = &minted_reward - &miner_reward - &explicit_reward;
    ensure!(
        explicit_reward == explicit_minted && burn_allocation == burn_minted,
        RewardDistributionError::AllocationMismatch
    );
    let weight_total: u128 = streams.iter().map(|s| u128::from(s.weight)).sum();
    let burn_weight = u64::try_from(weight_total)
        .ok()
        .and_then(|total| DENOM.checked_sub(total))
        .ok_or(RewardDistributionError::WeightsExceedDenominator)?;
    let amounts = RewardAmounts {
        minted_reward,
        miner_reward,
        explicit_reward,
        burn_allocation,
        ..Default::default()
    };
    Ok((streams, burn_weight, amounts))
}
