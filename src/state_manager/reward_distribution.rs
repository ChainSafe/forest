// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::state_computation::apply_block_messages_blocking;
use super::utils::structured;
use super::*;
use crate::blocks::CachingBlockHeader;
use crate::interpreter::{CalledAt, VMTrace};
use crate::rpc::state::{
    BlockReward, ExecutionTrace, ExplicitRewardDistribution, RecipientReward, RewardAmounts,
    StreamReward,
};
use anyhow::ensure;
use fil_actor_reward_state::v19::{DENOM, Ledger, State as RewardState, compute_weight};
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
    /// Fails when `tipset` cannot be executed, or was executed by a reward actor older than v19.
    pub async fn reward_distribution(
        &self,
        tipset: &Tipset,
    ) -> anyhow::Result<Arc<RewardDistribution>> {
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
        award_distribution(before.as_v19()?, after.as_v19()?, &ctx.state, header.epoch)?;
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

/// Reconstructs one award from the reward actor's state before it and after it.
///
/// Returns the reward of every stream, the burn weight and the minted amounts. The message
/// reward and the paid amounts are left at zero.
///
/// Taken from here:
/// <https://github.com/filecoin-project/lotus/blob/v1.37.0-rc2/chain/stmgr/rewards.go#L52-L107>
///
/// # Errors
/// Fails when the streams block of `after` is missing from `store` or fails the reward actor's
/// checks, when the reconstruction disagrees with the explicit and burn counters the award
/// committed, and when the stream weights exceed the denominator.
///
/// # Note:
/// This is not a direct translation from lotus in terms of the code structure so change it carefully.
fn award_distribution(
    before: &RewardState,
    after: &RewardState,
    store: &impl Blockstore,
    epoch: ChainEpoch,
) -> anyhow::Result<(Vec<StreamReward>, u64, RewardAmounts)> {
    let streams_block = store
        .get(&after.streams_root)?
        .with_context(|| format!("reward streams block {} not found", after.streams_root))?;
    // What every award runs on the streams block: decoding, then the structure and accounting
    // checks, which pair every explicit stream with one accrual row.
    let ledger = Ledger::decode_for_award(&streams_block, &after.accrued)?;

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
        "reward allocation does not match committed explicit and burn minting counters"
    );
    let weight_total: u128 = streams.iter().map(|s| u128::from(s.weight)).sum();
    let burn_weight = u64::try_from(weight_total)
        .ok()
        .and_then(|total| DENOM.checked_sub(total))
        .context("stream weights exceed the denominator")?;
    let amounts = RewardAmounts {
        minted_reward,
        miner_reward,
        explicit_reward,
        burn_allocation,
        ..Default::default()
    };
    Ok((streams, burn_weight, amounts))
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
pub(in crate::state_manager) mod tests {
    //! `award_distribution` is given the reward actor's state right before one award and right
    //! after it. From the difference it reports what the award did with the FIL it minted. An
    //! award is the reward actor minting the reward of one block and splitting it.
    //!
    //! Every amount is in attoFIL. Weights and shares are fractions of a whole, written with `percent`.

    use super::*;
    use crate::db::MemoryDB;
    use crate::rpc::state::{MessageTrace, ReturnTrace};
    use crate::shim::error::ExitCode;
    use crate::utils::db::CborStoreExt as _;
    use fil_actor_reward_state::v19::{
        ExplicitDistribution, RecipientShare, Stream, StreamAccrual, StreamsState, WeightRecord,
    };

    const MINER_STREAM: u64 = 1;
    const SERVICE_STREAM: u64 = 2;
    const SECOND_SERVICE_STREAM: u64 = 3;
    const ALICE: Address = Address::new_id(101);
    const BOB: Address = Address::new_id(102);
    const CAROL: Address = Address::new_id(103);
    const WRITER: Address = Address::new_id(104);
    /// The weights of these tests are the same at every epoch, so any epoch will do.
    const ANY_EPOCH: ChainEpoch = 0;

    /// The attoFIL that the pool of the explicit stream `stream_id` holds.
    #[derive(Clone)]
    struct StreamPool {
        stream_id: u64,
        amount: i64,
    }

    fn pool_of(stream_id: u64, amount: i64) -> StreamPool {
        StreamPool { stream_id, amount }
    }

    /// What the reward actor's state says at one moment, in attoFIL.
    #[derive(Clone)]
    struct Snapshot {
        /// All the FIL that awards have minted so far. How far an award moves it is what the
        /// award minted.
        total_minted_reward: i64,
        /// The part of it that awards put into pools. Only used to check the result.
        total_explicit_minted: i64,
        /// The part of it that awards burned. Only used to check the result.
        total_burn_minted: i64,
        /// What the pool of each explicit stream holds. Only the pools after the award are read.
        /// The state calls this field `accrued`.
        accrued_pools: Vec<StreamPool>,
    }

    /// The reward actor counts all the FIL its awards have minted. What one award minted is how
    /// far that counter moved. Here a single stream takes all of it for the miner.
    #[test]
    fn an_award_mints_what_the_minted_counter_grew_by() {
        let streams = vec![implicit_stream(MINER_STREAM, percent(100))];
        let before = Snapshot {
            total_minted_reward: 1_000,
            total_explicit_minted: 0,
            total_burn_minted: 0,
            accrued_pools: vec![],
        };
        // The award mints 100, all for the miner (implicit stream).
        let after = Snapshot {
            total_minted_reward: 1_100, // + 100
            total_explicit_minted: 0,
            total_burn_minted: 0,
            accrued_pools: vec![],
        };

        let (_, _, amounts) =
            call_award_distribution_at(ANY_EPOCH, streams, before, after).unwrap();

        assert_eq!(amounts.minted_reward, atto(100)); // 1_100 - 1_000
        assert_eq!(amounts.miner_reward, atto(100)); // 100% of 100
    }

    /// A stream takes its weight of the minted reward: its portion. The implicit stream is the
    /// one without a distribution, and its portion is the miner's. The weight that no stream has
    /// is the burn weight; that part of the minted reward is burned.
    #[test]
    fn a_stream_takes_its_weight_of_the_minted_reward() {
        let streams = vec![implicit_stream(MINER_STREAM, percent(60))];
        let before = Snapshot {
            total_minted_reward: 1_000,
            total_explicit_minted: 0,
            total_burn_minted: 0,
            accrued_pools: vec![],
        };
        // The award mints 100: 60 for the miner, 40 will be burned.
        let after = Snapshot {
            total_minted_reward: 1_100, // + 100
            total_explicit_minted: 0,
            total_burn_minted: 40, // + 40
            accrued_pools: vec![],
        };

        let (stream_rewards, burn_weight, amounts) =
            call_award_distribution_at(ANY_EPOCH, streams, before, after).unwrap();

        assert_eq!(
            stream_rewards,
            [StreamReward {
                id: MINER_STREAM,
                weight: percent(60),
                amount: atto(60), // its portion: 60% of 100
                distribution: None,
            }]
        );
        assert_eq!(amounts.miner_reward, atto(60)); // the implicit stream's portion
        assert_eq!(burn_weight, percent(40)); // 100% - 60%
        assert_eq!(amounts.burn_allocation, atto(40)); // 100 - 60
    }

    /// A weight can change from epoch to epoch. The split uses the weights of the award's epoch.
    #[test]
    fn weights_are_those_of_the_award_epoch() {
        let grows_one_percent_per_epoch = WeightRecord {
            v_start: 0,
            slope: percent(1) as i64,
            t_start: 0, // start epoch is 0
            floor: 0,
            cap: percent(100),
        };
        let streams = vec![Stream {
            id: MINER_STREAM,
            weight: grows_one_percent_per_epoch,
            distribution: None,
        }];
        let epoch = 30;
        let before = Snapshot {
            total_minted_reward: 1_000,
            total_explicit_minted: 0,
            total_burn_minted: 0,
            accrued_pools: vec![],
        };
        // At epoch 30 the award mints 100: 30 for the miner, 70 burned.
        let after = Snapshot {
            total_minted_reward: 1_100, // + 100
            total_explicit_minted: 0,
            total_burn_minted: 70, // + 70
            accrued_pools: vec![],
        };

        let (stream_rewards, burn_weight, amounts) =
            call_award_distribution_at(epoch, streams, before, after).unwrap();

        assert_eq!(stream_rewards.first().unwrap().weight, percent(30)); // 1% * 30 epochs
        assert_eq!(amounts.miner_reward, atto(30)); // 30% of 100
        assert_eq!(burn_weight, percent(70)); // 100% - 30%
    }

    /// An explicit stream pays nobody during the award. Its portion is added to the stream's
    /// pool, which the reward actor keeps for the stream's recipients.
    #[test]
    fn an_explicit_stream_saves_its_portion_in_its_pool() {
        let streams = vec![
            implicit_stream(MINER_STREAM, percent(60)),
            explicit_stream(SERVICE_STREAM, percent(40), &[(ALICE, percent(100))]),
        ];
        let before = Snapshot {
            total_minted_reward: 1_000,
            total_explicit_minted: 0,
            total_burn_minted: 0,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 0)],
        };
        // The award mints 100: 60 for the miner, 40 into the service pool.
        let after = Snapshot {
            total_minted_reward: 1_100, // + 100
            total_explicit_minted: 40,  // + 40
            total_burn_minted: 0,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 40)], // + 40
        };

        let (stream_rewards, _, amounts) =
            call_award_distribution_at(ANY_EPOCH, streams, before, after).unwrap();

        assert_eq!(portions(&stream_rewards), [60, 40]); // 60% (Miner) and 40% (ALICE) of 100
        assert_eq!(amounts.miner_reward, atto(60));
        assert_eq!(amounts.explicit_reward, atto(40)); // what went into pools
        assert_eq!(earned(&stream_rewards, SERVICE_STREAM), [40]); // the pool is all ALICE's
    }

    /// A pool belongs to the stream's recipients by their shares. A recipient's part of it is
    /// pool * share / sum of the shares.
    #[test]
    fn recipients_share_the_pool() {
        let shares = [(ALICE, percent(75)), (BOB, percent(25))];
        let streams = vec![
            implicit_stream(MINER_STREAM, percent(60)),
            explicit_stream(SERVICE_STREAM, percent(40), &shares),
        ];
        let before = Snapshot {
            total_minted_reward: 1_000,
            total_explicit_minted: 0,
            total_burn_minted: 0,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 0)],
        };
        // The award mints 100: 60 for the miner, 40 into the service pool.
        let after = Snapshot {
            total_minted_reward: 1_100, // + 100
            total_explicit_minted: 40,  // + 40
            total_burn_minted: 0,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 40)], // + 40
        };

        let (stream_rewards, _, _) =
            call_award_distribution_at(ANY_EPOCH, streams, before, after).unwrap();

        // ALICE's part of the pool: 40 * 75/100. BOB's: 40 * 25/100.
        assert_eq!(earned(&stream_rewards, SERVICE_STREAM), [30, 10]);
    }

    /// The next award adds to the same pool. A recipient's part of the pool grows with it, and
    /// that growth is what the award earned the recipient.
    #[test]
    fn the_next_award_adds_to_the_same_pool() {
        let shares = [(ALICE, percent(75)), (BOB, percent(25))];
        let streams = vec![
            implicit_stream(MINER_STREAM, percent(60)),
            explicit_stream(SERVICE_STREAM, percent(40), &shares),
        ];
        let before = Snapshot {
            total_minted_reward: 1_100,
            total_explicit_minted: 40,
            total_burn_minted: 0,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 40)],
        };
        // The award mints 100: 60 for the miner, 40 into the service pool.
        let after = Snapshot {
            total_minted_reward: 1_200, // + 100
            total_explicit_minted: 80,  // + 40
            total_burn_minted: 0,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 80)], // + 40
        };

        let (stream_rewards, _, _) =
            call_award_distribution_at(ANY_EPOCH, streams, before, after).unwrap();

        // ALICE's part: 40 * 75/100 = 30 before, 80 * 75/100 = 60 after.
        // BOB's part: 40 * 25/100 = 10 before, 80 * 25/100 = 20 after.
        assert_eq!(earned(&stream_rewards, SERVICE_STREAM), [30, 10]); // 60 - 30, 20 - 10
    }

    /// Before it adds to a pool, an award can first empty it: what the pool held is set aside
    /// for its recipients and the pool restarts at zero (a fold). The state before the award
    /// still shows the old pool then. So the pool of the earlier state is never read: the pool
    /// before the addition is the pool after the award minus what the award added.
    #[test]
    fn the_pool_before_is_worked_out_from_the_pool_after() {
        let streams = vec![
            implicit_stream(MINER_STREAM, percent(60)),
            explicit_stream(SERVICE_STREAM, percent(40), &[(ALICE, percent(100))]),
        ];
        let before = Snapshot {
            total_minted_reward: 1_000,
            total_explicit_minted: 500,
            total_burn_minted: 0,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 500)],
        };
        // Inside the award, first: the service pool is emptied and its 500 are set aside for
        // ALICE. No state stores this moment, so `award_distribution` is never given it.
        let emptied = Snapshot {
            total_minted_reward: 1_000,
            total_explicit_minted: 500,
            total_burn_minted: 0,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 0)],
        };
        // Inside the award, then: it mints 100: 60 for the miner, 40 into the emptied pool.
        let after = Snapshot {
            total_minted_reward: 1_100, // + 100
            total_explicit_minted: 540, // + 40
            total_burn_minted: 0,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 40)], // 0 + 40
        };

        let from_before =
            call_award_distribution_at(ANY_EPOCH, streams.clone(), before, after.clone()).unwrap();
        let from_emptied = call_award_distribution_at(ANY_EPOCH, streams, emptied, after).unwrap();

        // Given `before`, the function reports what it would report for `emptied`.
        assert_eq!(from_before, from_emptied);
        let (stream_rewards, _, _) = from_before;
        // ALICE's part of the pool: 0 once it is emptied, 40 after the award. Not 40 - 500.
        assert_eq!(earned(&stream_rewards, SERVICE_STREAM), [40]);
    }

    /// Shares need not add up to 100%. The part of a portion that no recipient has is burned
    /// instead of going into the pool; that fraction is the stream's burn share. A recipient's
    /// part of the pool is still pool * share / sum of the shares.
    #[test]
    fn shares_that_nobody_has_are_burned() {
        let shares = [(ALICE, percent(50)), (BOB, percent(25))];
        let streams = vec![
            implicit_stream(MINER_STREAM, percent(60)),
            explicit_stream(SERVICE_STREAM, percent(40), &shares),
        ];
        let before = Snapshot {
            total_minted_reward: 1_000,
            total_explicit_minted: 0,
            total_burn_minted: 0,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 0)],
        };
        // The award mints 100: 60 for the miner, 30 into the service pool, 10 burned.
        let after = Snapshot {
            total_minted_reward: 1_100,                       // + 100
            total_explicit_minted: 30,                        // + 30
            total_burn_minted: 10,                            // + 10
            accrued_pools: vec![pool_of(SERVICE_STREAM, 30)], // + 30
        };

        let (stream_rewards, _, amounts) =
            call_award_distribution_at(ANY_EPOCH, streams, before, after).unwrap();

        let service = distribution_of(&stream_rewards, SERVICE_STREAM);
        assert_eq!(service.burn_share, percent(25)); // 100% - 50% - 25%
        assert_eq!(amounts.explicit_reward, atto(30)); // into the pool: 75% of the portion of 40
        assert_eq!(service.burn_amount, atto(10)); // burned: the other 25% of 40 (explicit stream)
        assert_eq!(amounts.burn_allocation, atto(10)); // 100 - 60 - 30
        assert_eq!(earned(&stream_rewards, SERVICE_STREAM), [20, 10]); // 30 * 50/75, 30 * 25/75
    }

    /// Only the minted counter is read. The split is worked out from it, and the explicit and
    /// burn counters must have moved by exactly what the split puts into pools and burns.
    /// Otherwise the function returns an error instead of a wrong report.
    #[test]
    fn the_split_must_match_the_explicit_and_burn_counters() {
        // The award of the test above: 30 into the pool and 10 burned. Each run has one of the
        // two counters off by one.
        for (explicit_after, burn_after) in [(29, 10), (30, 9)] {
            let shares = [(ALICE, percent(50)), (BOB, percent(25))];
            let streams = vec![
                implicit_stream(MINER_STREAM, percent(60)),
                explicit_stream(SERVICE_STREAM, percent(40), &shares),
            ];
            let before = Snapshot {
                total_minted_reward: 1_000,
                total_explicit_minted: 0,
                total_burn_minted: 0,
                accrued_pools: vec![pool_of(SERVICE_STREAM, 0)],
            };
            let after = Snapshot {
                total_minted_reward: 1_100,
                total_explicit_minted: explicit_after,
                total_burn_minted: burn_after,
                accrued_pools: vec![pool_of(SERVICE_STREAM, 30)],
            };

            let error = call_award_distribution_at(ANY_EPOCH, streams, before, after).unwrap_err();

            assert_eq!(
                error.to_string(),
                "reward allocation does not match committed explicit and burn minting counters"
            );
        }
    }

    /// Without an implicit stream none of the minted reward is the miner's.
    #[test]
    fn without_an_implicit_stream_the_miner_gets_nothing() {
        let streams = vec![explicit_stream(
            SERVICE_STREAM,
            percent(50),
            &[(ALICE, percent(100))],
        )];
        let before = Snapshot {
            total_minted_reward: 1_000,
            total_explicit_minted: 0,
            total_burn_minted: 0,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 0)],
        };
        // The award mints 10: 5 into the service pool, 5 burned.
        let after = Snapshot {
            total_minted_reward: 1_010,                      // + 10
            total_explicit_minted: 5,                        // + 5
            total_burn_minted: 5,                            // + 5
            accrued_pools: vec![pool_of(SERVICE_STREAM, 5)], // + 5
        };

        let (stream_rewards, burn_weight, amounts) =
            call_award_distribution_at(ANY_EPOCH, streams, before, after).unwrap();

        assert_eq!(amounts.miner_reward, atto(0));
        assert_eq!(earned(&stream_rewards, SERVICE_STREAM), [5]); // 50% of 10, all ALICE's
        assert_eq!(burn_weight, percent(50)); // 100% - 50%
        assert_eq!(amounts.burn_allocation, atto(5)); // 10 - 0 - 5
    }

    /// An attoFIL cannot be split, so every amount is rounded down: each portion first, then
    /// what goes into a pool. What the rounding takes off is burned.
    #[test]
    fn amounts_are_rounded_down_to_whole_attofil() {
        let streams = vec![
            implicit_stream(MINER_STREAM, percent(41)),
            explicit_stream(SERVICE_STREAM, percent(59), &[(ALICE, percent(75))]),
        ];
        let before = Snapshot {
            total_minted_reward: 1_000,
            total_explicit_minted: 0,
            total_burn_minted: 0,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 0)],
        };
        // The award mints 10: 4 for the miner, 3 into the service pool, 3 burned.
        let after = Snapshot {
            total_minted_reward: 1_010,                      // + 10
            total_explicit_minted: 3,                        // + 3
            total_burn_minted: 3,                            // + 3
            accrued_pools: vec![pool_of(SERVICE_STREAM, 3)], // + 3
        };

        let (stream_rewards, _, amounts) =
            call_award_distribution_at(ANY_EPOCH, streams, before, after).unwrap();

        assert_eq!(portions(&stream_rewards), [4, 5]); // 41% (Miner) and 59% (ALICE) of 10 = 4.1 and 5.9
        assert_eq!(amounts.explicit_reward, atto(3)); // into the pool: 75% of 5 = 3.75
        let service = distribution_of(&stream_rewards, SERVICE_STREAM);
        assert_eq!(service.burn_amount, atto(2)); // 5 - 3
        assert_eq!(amounts.burn_allocation, atto(3)); // 10 - 4 - 3
    }

    /// A recipient's part of the pool is rounded down as well, so a pool can hold attoFIL that
    /// are in nobody's part yet. The rounding adjustment is what the award added to the pool
    /// minus what the recipients earned.
    #[test]
    fn a_part_of_the_pool_is_rounded_down_too() {
        let halves = [(ALICE, percent(50)), (BOB, percent(50))];
        let streams = vec![explicit_stream(SERVICE_STREAM, percent(100), &halves)];
        let before = Snapshot {
            total_minted_reward: 1_000,
            total_explicit_minted: 0,
            total_burn_minted: 0,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 0)],
        };
        // The award mints 1, all into the service pool.
        let after = Snapshot {
            total_minted_reward: 1_001, // + 1
            total_explicit_minted: 1,   // + 1
            total_burn_minted: 0,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 1)], // + 1
        };

        let (stream_rewards, _, _) =
            call_award_distribution_at(ANY_EPOCH, streams, before, after).unwrap();

        // Half of the pool for ALICE and BOB: 0 before, 0.5 after, rounded down to 0.
        assert_eq!(earned(&stream_rewards, SERVICE_STREAM), [0, 0]);
        let service = distribution_of(&stream_rewards, SERVICE_STREAM);
        assert_eq!(service.rounding_adjustment, atto(1)); // 1 added - 0 earned
    }

    /// The next award makes the pool a size the shares divide evenly, so the attoFIL the award
    /// before left in it is earned now. The recipients earn more than this award added, and the
    /// rounding adjustment is negative.
    #[test]
    fn attofil_left_in_the_pool_are_earned_by_a_later_award() {
        let halves = [(ALICE, percent(50)), (BOB, percent(50))];
        let streams = vec![explicit_stream(SERVICE_STREAM, percent(100), &halves)];
        let before = Snapshot {
            total_minted_reward: 1_001,
            total_explicit_minted: 1,
            total_burn_minted: 0,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 1)],
        };
        // The award mints 1, all into the service pool.
        let after = Snapshot {
            total_minted_reward: 1_002, // + 1
            total_explicit_minted: 2,   // + 1
            total_burn_minted: 0,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 2)], // + 1
        };

        let (stream_rewards, _, _) =
            call_award_distribution_at(ANY_EPOCH, streams, before, after).unwrap();

        // Half of the pool: 0.5 before, rounded down to 0, and 1 after.
        assert_eq!(earned(&stream_rewards, SERVICE_STREAM), [1, 1]); // 1 - 0 each
        let service = distribution_of(&stream_rewards, SERVICE_STREAM);
        assert_eq!(service.rounding_adjustment, atto(-1)); // 1 added - 2 earned
    }

    /// Each explicit stream has its own pool and its own recipients.
    #[test]
    fn every_explicit_stream_has_its_own_pool() {
        let second_shares = [(BOB, percent(50)), (CAROL, percent(25))];
        let streams = vec![
            implicit_stream(MINER_STREAM, percent(50)),
            explicit_stream(SERVICE_STREAM, percent(29), &[(ALICE, percent(100))]),
            explicit_stream(SECOND_SERVICE_STREAM, percent(21), &second_shares),
        ];
        let before = Snapshot {
            total_minted_reward: 1_000,
            total_explicit_minted: 0,
            total_burn_minted: 0,
            accrued_pools: vec![
                pool_of(SERVICE_STREAM, 0),
                pool_of(SECOND_SERVICE_STREAM, 0),
            ],
        };
        // The award mints 1001: 500 for the miner, 290 and 157 into the pools, 54 burned.
        let after = Snapshot {
            total_minted_reward: 2_001, // + 1001
            total_explicit_minted: 447, // + 447
            total_burn_minted: 54,      // + 54
            accrued_pools: vec![
                pool_of(SERVICE_STREAM, 290),        // + 290
                pool_of(SECOND_SERVICE_STREAM, 157), // + 157
            ],
        };

        let (stream_rewards, burn_weight, amounts) =
            call_award_distribution_at(ANY_EPOCH, streams, before, after).unwrap();

        // 50%, 29% and 21% of 1001 = 500.5, 290.29 and 210.21.
        assert_eq!(portions(&stream_rewards), [500, 290, 210]);
        assert_eq!(amounts.miner_reward, atto(500));

        assert_eq!(earned(&stream_rewards, SERVICE_STREAM), [290]); // the pool is all ALICE's

        // Into the second pool: 75% of 210 = 157.5.
        // BOB's part of it: 157 * 50/75 = 104.67. CAROL's: 157 * 25/75 = 52.33.
        assert_eq!(earned(&stream_rewards, SECOND_SERVICE_STREAM), [104, 52]);
        let second = distribution_of(&stream_rewards, SECOND_SERVICE_STREAM);
        assert_eq!(second.burn_amount, atto(53)); // 210 - 157
        assert_eq!(second.rounding_adjustment, atto(1)); // 157 added - 104 - 52 earned

        assert_eq!(amounts.explicit_reward, atto(447)); // 290 + 157
        assert_eq!(amounts.burn_allocation, atto(54)); // 1001 - 500 - 447
        assert_award_conserved(&stream_rewards, burn_weight, &amounts);
    }

    /// All the rules above in one award, with counters and a pool that did not start at zero.
    #[test]
    fn one_award_with_every_rule() {
        let shares = [(ALICE, percent(50)), (BOB, percent(25))];
        let streams = vec![
            implicit_stream(MINER_STREAM, percent(60)),
            explicit_stream(SERVICE_STREAM, percent(30), &shares),
        ];
        let before = Snapshot {
            total_minted_reward: 1_000,
            total_explicit_minted: 200,
            total_burn_minted: 100,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 30)],
        };
        // The award mints 100: 60 for the miner, 22 into the service pool, 18 burned.
        let after = Snapshot {
            total_minted_reward: 1_100,                       // + 100
            total_explicit_minted: 222,                       // + 22
            total_burn_minted: 118,                           // + 18
            accrued_pools: vec![pool_of(SERVICE_STREAM, 52)], // + 22
        };

        let (stream_rewards, burn_weight, amounts) =
            call_award_distribution_at(ANY_EPOCH, streams, before, after).unwrap();

        assert_eq!(
            amounts,
            RewardAmounts {
                minted_reward: atto(100),  // 1_100 - 1_000
                miner_reward: atto(60),    // 60% of 100
                explicit_reward: atto(22), // 75% of the service portion of 30 = 22.5
                burn_allocation: atto(18), // 100 - 60 - 22
                // The message reward and the amounts paid are filled in by the caller.
                ..Default::default()
            }
        );
        assert_eq!(burn_weight, percent(10)); // 100% - 60% - 30%
        assert_eq!(
            stream_rewards,
            [
                StreamReward {
                    id: MINER_STREAM,
                    weight: percent(60),
                    amount: atto(60), // 60% of 100
                    distribution: None,
                },
                StreamReward {
                    id: SERVICE_STREAM,
                    weight: percent(30),
                    amount: atto(30), // 30% of 100
                    distribution: Some(ExplicitRewardDistribution {
                        writer: WRITER, // the address that may change the shares
                        recipients: vec![
                            RecipientReward {
                                recipient: ALICE,
                                share: percent(50),
                                // Her part: 30 * 50/75 = 20 before, 52 * 50/75 = 34.67 after.
                                earned_amount: atto(14), // 34 - 20
                            },
                            RecipientReward {
                                recipient: BOB,
                                share: percent(25),
                                // His part: 30 * 25/75 = 10 before, 52 * 25/75 = 17.33 after.
                                earned_amount: atto(7), // 17 - 10
                            },
                        ],
                        burn_share: percent(25),      // 100% - 50% - 25%
                        burn_amount: atto(8),         // 30 - 22
                        rounding_adjustment: atto(1), // 22 added - 14 - 7 earned
                    }),
                },
            ]
        );
        // 100 = 60 miner + 14 ALICE + 7 BOB + 1 in the pool for later + 18 burned
        assert_award_conserved(&stream_rewards, burn_weight, &amounts);
    }

    /// An explicit stream without recipients has a burn share of 100%: its whole portion is
    /// burned and its pool stays empty.
    #[test]
    fn a_stream_without_recipients_has_its_portion_burned() {
        let streams = vec![explicit_stream(SERVICE_STREAM, percent(100), &[])];
        let before = Snapshot {
            total_minted_reward: 1_000,
            total_explicit_minted: 0,
            total_burn_minted: 0,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 0)],
        };
        // The award mints 10, all burned.
        let after = Snapshot {
            total_minted_reward: 1_010, // + 10
            total_explicit_minted: 0,
            total_burn_minted: 10, // + 10
            accrued_pools: vec![pool_of(SERVICE_STREAM, 0)],
        };

        let (stream_rewards, _, amounts) =
            call_award_distribution_at(ANY_EPOCH, streams, before, after).unwrap();

        let service = distribution_of(&stream_rewards, SERVICE_STREAM);
        assert!(service.recipients.is_empty());
        assert_eq!(service.burn_share, percent(100));
        assert_eq!(service.burn_amount, atto(10)); // the whole portion: 100% of 10
        assert_eq!(amounts.burn_allocation, atto(10));
    }

    /// Without streams the burn weight is 100%: the whole minted reward is burned.
    #[test]
    fn without_streams_everything_is_burned() {
        let before = Snapshot {
            total_minted_reward: 1_000,
            total_explicit_minted: 0,
            total_burn_minted: 0,
            accrued_pools: vec![],
        };
        // The award mints 10, all burned.
        let after = Snapshot {
            total_minted_reward: 1_010, // + 10
            total_explicit_minted: 0,
            total_burn_minted: 10, // + 10
            accrued_pools: vec![],
        };

        let (stream_rewards, burn_weight, amounts) =
            call_award_distribution_at(ANY_EPOCH, vec![], before, after).unwrap();

        assert!(stream_rewards.is_empty());
        assert_eq!(burn_weight, percent(100));
        assert_eq!(amounts.burn_allocation, atto(10));
    }

    /// When the reward actor cannot split a reward safely it mints nothing and leaves its state
    /// as it was (`no_award`). The same rules then give zero for every amount, and the streams
    /// are still listed.
    #[test]
    fn an_award_that_mints_nothing_reports_zero_amounts() {
        let halves = [(ALICE, percent(50)), (BOB, percent(50))];
        let streams = vec![
            implicit_stream(MINER_STREAM, percent(60)),
            explicit_stream(SERVICE_STREAM, percent(40), &halves),
        ];
        let before = Snapshot {
            total_minted_reward: 1_000,
            total_explicit_minted: 200,
            total_burn_minted: 100,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 13)],
        };
        // The award mints nothing and changes nothing.
        let after = Snapshot {
            total_minted_reward: 1_000,
            total_explicit_minted: 200,
            total_burn_minted: 100,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 13)],
        };

        let (stream_rewards, _, amounts) =
            call_award_distribution_at(ANY_EPOCH, streams, before, after).unwrap();

        assert_eq!(amounts, RewardAmounts::default()); // every amount is zero
        assert_eq!(portions(&stream_rewards), [0, 0]);
        // Half of the pool: 6.5 before and after, rounded down to 6.
        assert_eq!(earned(&stream_rewards, SERVICE_STREAM), [0, 0]); // 6 - 6 each
    }

    /// Weights that add up to more than 100% are an error.
    #[test]
    fn weights_above_100_percent_are_an_error() {
        // Weights: 100%, and one part in `DENOM` more.
        let streams = vec![
            implicit_stream(MINER_STREAM, percent(100)),
            explicit_stream(SERVICE_STREAM, 1, &[(ALICE, percent(100))]),
        ];
        let before = Snapshot {
            total_minted_reward: 1_000,
            total_explicit_minted: 0,
            total_burn_minted: 0,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 0)],
        };
        // The reward actor mints nothing with such weights, so nothing changed.
        let after = Snapshot {
            total_minted_reward: 1_000,
            total_explicit_minted: 0,
            total_burn_minted: 0,
            accrued_pools: vec![pool_of(SERVICE_STREAM, 0)],
        };

        let error = call_award_distribution_at(ANY_EPOCH, streams, before, after).unwrap_err();

        assert_eq!(error.to_string(), "stream weights exceed the denominator");
    }

    /// The reward actor checks its stream data before every award, and `award_distribution`
    /// runs the same checks. Data that fails them is an error. Here the explicit stream has no
    /// pool.
    #[test]
    fn stream_data_the_reward_actor_rejects_is_an_error() {
        let streams = vec![explicit_stream(
            SERVICE_STREAM,
            percent(100),
            &[(ALICE, percent(100))],
        )];
        let before = Snapshot {
            total_minted_reward: 1_000,
            total_explicit_minted: 0,
            total_burn_minted: 0,
            accrued_pools: vec![],
        };
        // Nothing changed.
        let after = Snapshot {
            total_minted_reward: 1_000,
            total_explicit_minted: 0,
            total_burn_minted: 0,
            accrued_pools: vec![],
        };

        let error = call_award_distribution_at(ANY_EPOCH, streams, before, after).unwrap_err();

        assert_eq!(
            error.to_string(),
            "explicit-stream accrual IDs do not match live explicit streams"
        );
    }

    /// A call the reward actor made while it ran the award, sending `value` to `to`.
    fn transfer(to: Address, value: i64, exit_code: u32) -> ExecutionTrace {
        ExecutionTrace {
            msg: MessageTrace {
                from: Address::REWARD_ACTOR,
                to,
                value: atto(value),
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

        assert_eq!(miner_paid, atto(107)); // the failed 41 is left out
        assert_eq!(burn_paid, atto(22)); // 2 + 20; the nested 13 and the failed 99 are left out
    }

    // Helpers.

    /// `n` percent as a weight or a share, which are whole numbers out of `DENOM (10^18)`.
    fn percent(n: u64) -> u64 {
        DENOM / 100 * n
    }

    fn atto(amount: i64) -> TokenAmount {
        TokenAmount::from_atto(amount)
    }

    fn int(amount: &TokenAmount) -> i64 {
        amount.atto().try_into().unwrap()
    }

    /// A weight record that gives `weight` at every epoch.
    fn constant(weight: u64) -> WeightRecord {
        WeightRecord {
            v_start: weight,
            slope: 0,
            t_start: 0,
            floor: weight,
            cap: weight,
        }
    }

    /// The stream that pays the block's miner `weight` of the minted reward. It has no
    /// distribution.
    fn implicit_stream(id: u64, weight: u64) -> Stream {
        Stream {
            id,
            weight: constant(weight),
            distribution: None,
        }
    }

    /// A stream that takes `weight` of the minted reward for the recipients in `shares`.
    fn explicit_stream(id: u64, weight: u64, shares: &[(Address, u64)]) -> Stream {
        let shares = shares
            .iter()
            .map(|&(recipient, share)| RecipientShare {
                recipient: recipient.into(),
                share,
            })
            .collect();
        Stream {
            id,
            weight: constant(weight),
            distribution: Some(ExplicitDistribution {
                writer: WRITER.into(),
                shares,
                payable: Default::default(),
                claimed_period: Default::default(),
            }),
        }
    }

    fn call_award_distribution_at(
        epoch: ChainEpoch,
        streams: Vec<Stream>,
        before: Snapshot,
        after: Snapshot,
    ) -> anyhow::Result<(Vec<StreamReward>, u64, RewardAmounts)> {
        let db = MemoryDB::default();
        let stored_streams = StreamsState {
            streams,
            ..Default::default()
        };
        let streams_root = db.put_cbor_default(&stored_streams).unwrap();
        let before = reward_state(before, streams_root);
        let after = reward_state(after, streams_root);
        award_distribution(&before, &after, &db, epoch)
    }

    /// The reward actor's state holding the values of `snapshot`.
    fn reward_state(snapshot: Snapshot, streams_root: Cid) -> RewardState {
        let accrued = snapshot.accrued_pools.iter().map(|pool| StreamAccrual {
            id: pool.stream_id,
            amount: atto(pool.amount).into(),
        });
        RewardState {
            total_minted_reward: atto(snapshot.total_minted_reward).into(),
            total_explicit_minted: atto(snapshot.total_explicit_minted).into(),
            total_burn_minted: atto(snapshot.total_burn_minted).into(),
            accrued: accrued.collect(),
            streams_root,
            ..Default::default()
        }
    }

    /// The portion of every stream, in the order the streams are stored.
    fn portions(stream_rewards: &[StreamReward]) -> Vec<i64> {
        stream_rewards.iter().map(|s| int(&s.amount)).collect()
    }

    /// The distribution reported for the explicit stream `stream_id`.
    fn distribution_of(
        stream_rewards: &[StreamReward],
        stream_id: u64,
    ) -> &ExplicitRewardDistribution {
        let stream = stream_rewards.iter().find(|s| s.id == stream_id).unwrap();
        stream.distribution.as_ref().unwrap()
    }

    /// What the award earned each recipient of the explicit stream `stream_id`.
    fn earned(stream_rewards: &[StreamReward], stream_id: u64) -> Vec<i64> {
        let recipients = distribution_of(stream_rewards, stream_id).recipients.iter();
        recipients.map(|r| int(&r.earned_amount)).collect()
    }

    /// Asserts the sums that hold for every award.
    ///
    /// Minted = miner + explicit + burn. Weights + burn weight = 100%. For an explicit stream,
    /// shares + burn share = 100% and portion = earned + burned + rounding adjustment.
    pub(in crate::state_manager) fn assert_award_conserved(
        streams: &[StreamReward],
        burn_weight: u64,
        amounts: &RewardAmounts,
    ) {
        assert_eq!(
            amounts.minted_reward,
            &amounts.miner_reward + &amounts.explicit_reward + &amounts.burn_allocation
        );
        let weights: u64 = streams.iter().map(|stream| stream.weight).sum();
        assert_eq!(weights + burn_weight, DENOM);

        for stream in streams {
            let Some(distribution) = &stream.distribution else {
                continue;
            };
            let mut shares = 0;
            let mut earned = TokenAmount::zero();
            for recipient in &distribution.recipients {
                shares += recipient.share;
                earned += recipient.earned_amount.clone();
            }
            assert_eq!(shares + distribution.burn_share, DENOM);
            assert_eq!(
                stream.amount,
                earned + &distribution.burn_amount + &distribution.rounding_adjustment
            );
        }
    }
}
