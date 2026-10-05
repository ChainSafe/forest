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
    /// traced execution.
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
        ensure!(
            blocks.len() == tipset.len(),
            "{} reward messages for {} blocks",
            blocks.len(),
            tipset.len()
        );

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
    let params: AwardBlockRewardParams = ctx.message.message().params.deserialize()?;
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
/// Port of Lotus's `rewardDistributionForAward`:
/// <https://github.com/filecoin-project/lotus/blob/v1.37.0-rc2/chain/stmgr/rewards.go#L52-L107>
///
/// # Errors
/// Fails when the streams block of `after` is missing from `store` or fails the reward actor's
/// checks, when the reconstruction disagrees with the explicit and burn counters the award
/// committed, and when the stream weights exceed the denominator.
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

    let minted = TokenAmount::from(&after.total_minted_reward)
        - TokenAmount::from(&before.total_minted_reward);
    let explicit_minted = TokenAmount::from(&after.total_explicit_minted)
        - TokenAmount::from(&before.total_explicit_minted);
    let burn_minted =
        TokenAmount::from(&after.total_burn_minted) - TokenAmount::from(&before.total_burn_minted);

    let mut miner_reward = TokenAmount::zero();
    let mut explicit_reward = TokenAmount::zero();
    let mut streams = Vec::with_capacity(ledger.streams().streams.len());

    for stream in &ledger.streams().streams {
        let weight = compute_weight(&stream.weight, epoch);
        let portion = (&minted * weight).div_floor(DENOM);
        let distribution = match &stream.distribution {
            None => {
                miner_reward += portion.clone();
                None
            }
            Some(distribution) => {
                let share_total = distribution.share_total();
                let accrued = (&portion * share_total).div_floor(DENOM);
                let pool: TokenAmount = after
                    .accrued
                    .iter()
                    .find(|accrual| accrual.id == stream.id)
                    .map(|accrual| (&accrual.amount).into())
                    .expect("accounting checks: every explicit stream has an accrual row");
                // A fold in this award empties the pool before the accrual, so the pool the
                // accrual was added to is derived from the pool after it, never read from the
                // earlier state.
                let pool_before = &pool - &accrued;

                let mut rounding_adjustment = accrued.clone();
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
                let burn_amount = &portion - &accrued;
                explicit_reward += accrued;
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
            weight,
            amount: portion,
            distribution,
        });
    }

    let burn_allocation = &minted - &miner_reward - &explicit_reward;
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
        minted_reward: minted,
        miner_reward,
        explicit_reward,
        burn_allocation,
        ..Default::default()
    };
    Ok((streams, burn_weight, amounts))
}

/// Sums what the successful calls made by the reward actor itself transferred to `miner` and
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
    use super::*;
    use crate::db::MemoryDB;
    use crate::rpc::state::{MessageTrace, ReturnTrace};
    use crate::shim::error::ExitCode;
    use crate::utils::db::CborStoreExt as _;
    use fil_actor_reward_state::v19::{
        ExplicitDistribution, RecipientShare, Stream, StreamAccrual, StreamsState, WeightRecord,
    };

    const D: u64 = DENOM;
    const ALICE: Address = Address::new_id(101);
    const BOB: Address = Address::new_id(102);
    const CAROL: Address = Address::new_id(103);
    const WRITER: Address = Address::new_id(104);

    fn atto(amount: i64) -> TokenAmount {
        TokenAmount::from_atto(amount)
    }

    fn int(amount: &TokenAmount) -> i64 {
        amount.atto().try_into().unwrap()
    }

    /// A stream as the reward actor stores it, with the pool its accrual row holds after the
    /// award. The implicit stream has no accrual row.
    type StoredStream = (Stream, Option<i64>);

    /// A weight that is `weight` at every epoch.
    fn constant(weight: u64) -> WeightRecord {
        WeightRecord {
            v_start: weight,
            slope: 0,
            t_start: 0,
            floor: weight,
            cap: weight,
        }
    }

    fn implicit(id: u64, weight: u64) -> StoredStream {
        let stream = Stream {
            id,
            weight: constant(weight),
            distribution: None,
        };
        (stream, None)
    }

    /// An explicit stream whose pool holds `accrued` after the award.
    fn explicit(id: u64, weight: u64, shares: &[(Address, u64)], accrued: i64) -> StoredStream {
        let mut distribution = ExplicitDistribution {
            writer: WRITER.into(),
            shares: vec![],
            payable: Default::default(),
            claimed_period: Default::default(),
        };
        for &(recipient, share) in shares {
            distribution.shares.push(RecipientShare {
                recipient: recipient.into(),
                share,
            });
        }
        let stream = Stream {
            id,
            weight: constant(weight),
            distribution: Some(distribution),
        };
        (stream, Some(accrued))
    }

    /// The reward actor's state before an award and after it, the award having minted `minted`,
    /// of which `explicit` accrued to explicit streams and `burned` was allocated to burn.
    fn states(
        db: &MemoryDB,
        streams: Vec<StoredStream>,
        minted: i64,
        explicit: i64,
        burned: i64,
    ) -> (RewardState, RewardState) {
        let mut stored = StreamsState::default();
        let mut accrued = vec![];
        for (stream, pool) in streams {
            if let Some(pool) = pool {
                accrued.push(StreamAccrual {
                    id: stream.id,
                    amount: atto(pool).into(),
                });
            }
            stored.streams.push(stream);
        }
        let before = RewardState {
            total_minted_reward: atto(1_000).into(),
            total_burn_minted: atto(100).into(),
            total_explicit_minted: atto(100).into(),
            ..Default::default()
        };
        let after = RewardState {
            total_minted_reward: atto(1_000 + minted).into(),
            total_burn_minted: atto(100 + burned).into(),
            total_explicit_minted: atto(100 + explicit).into(),
            accrued,
            streams_root: db.put_cbor_default(&stored).unwrap(),
            ..Default::default()
        };
        (before, after)
    }

    /// Asserts the identities that hold for every award.
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

    /// One award: the stored streams after it, the counter increases it committed and the
    /// expected reconstruction.
    struct Award {
        name: &'static str,
        streams: Vec<StoredStream>,
        minted: i64,
        explicit: i64,
        burned: i64,
        miner: i64,
        burn_weight: u64,
        /// Per explicit stream, in stored order: its burn amount, its rounding adjustment and
        /// what each recipient earned.
        explicit_streams: Vec<(i64, i64, Vec<i64>)>,
    }

    #[test]
    fn award_distribution_reconstructs_the_actor_split() {
        let halves = [(ALICE, D / 2), (BOB, D / 2)];
        // Half to the miner, 29% to Alice, and 21% to Bob and Carol, who leave a quarter of
        // their stream unassigned.
        let three_streams = |pool_2, pool_3| {
            vec![
                implicit(1, D / 2),
                explicit(2, D / 100 * 29, &[(ALICE, D)], pool_2),
                explicit(3, D / 100 * 21, &[(BOB, D / 2), (CAROL, D / 4)], pool_3),
            ]
        };
        let awards = [
            Award {
                name: "unassigned weight and shares both burn",
                streams: vec![
                    implicit(7, 3 * D / 5),
                    explicit(8, 3 * D / 10, &[(ALICE, D / 2), (BOB, D / 4)], 52),
                ],
                minted: 100,
                explicit: 22,
                burned: 18,
                miner: 60,
                burn_weight: D / 10,
                explicit_streams: vec![(8, 1, vec![14, 7])],
            },
            Award {
                name: "the stream portion is floored before the share total is applied",
                streams: vec![explicit(4, D / 100 * 59, &[(ALICE, D / 4 * 3)], 3)],
                minted: 10,
                explicit: 3,
                burned: 7,
                miner: 0,
                burn_weight: D / 100 * 41,
                explicit_streams: vec![(2, 0, vec![3])],
            },
            Award {
                name: "earlier rounding dust becomes earned",
                streams: vec![explicit(2, D, &halves, 2)],
                minted: 1,
                explicit: 1,
                burned: 0,
                miner: 0,
                burn_weight: 0,
                explicit_streams: vec![(0, -1, vec![1, 1])],
            },
            Award {
                name: "a fold in this award resets the pool",
                streams: vec![explicit(2, D, &halves, 1)],
                minted: 1,
                explicit: 1,
                burned: 0,
                miner: 0,
                burn_weight: 0,
                explicit_streams: vec![(0, 1, vec![0, 0])],
            },
            Award {
                name: "no implicit stream",
                streams: vec![explicit(9, D / 2, &[(ALICE, D)], 5)],
                minted: 10,
                explicit: 5,
                burned: 5,
                miner: 0,
                burn_weight: D / 2,
                explicit_streams: vec![(0, 0, vec![5])],
            },
            Award {
                name: "empty share map burns the stream",
                streams: vec![explicit(2, D, &[], 0)],
                minted: 10,
                explicit: 0,
                burned: 10,
                miner: 0,
                burn_weight: 0,
                explicit_streams: vec![(10, 0, vec![])],
            },
            Award {
                name: "empty schedule burns everything",
                streams: vec![],
                minted: 10,
                explicit: 0,
                burned: 10,
                miner: 0,
                burn_weight: D,
                explicit_streams: vec![],
            },
            Award {
                name: "gas-only award",
                streams: vec![explicit(2, D, &halves, 13)],
                minted: 0,
                explicit: 0,
                burned: 0,
                miner: 0,
                burn_weight: 0,
                explicit_streams: vec![(0, 0, vec![0, 0])],
            },
            Award {
                name: "two explicit streams with a residual",
                streams: three_streams(290, 157),
                minted: 1001,
                explicit: 447,
                burned: 54,
                miner: 500,
                burn_weight: 0,
                explicit_streams: vec![(0, 0, vec![290]), (53, 1, vec![104, 52])],
            },
            Award {
                name: "two explicit streams with pools carried over",
                streams: three_streams(295, 159),
                minted: 1001,
                explicit: 447,
                burned: 54,
                miner: 500,
                burn_weight: 0,
                explicit_streams: vec![(0, 0, vec![290]), (53, -1, vec![105, 53])],
            },
        ];

        for award in awards {
            let name = award.name;
            let db = MemoryDB::default();
            let (before, after) = states(
                &db,
                award.streams,
                award.minted,
                award.explicit,
                award.burned,
            );

            let (streams, burn_weight, amounts) = award_distribution(&before, &after, &db, 0)
                .unwrap_or_else(|e| panic!("{name}: {e}"));

            assert_eq!(int(&amounts.minted_reward), award.minted, "{name}");
            assert_eq!(int(&amounts.miner_reward), award.miner, "{name}");
            assert_eq!(int(&amounts.explicit_reward), award.explicit, "{name}");
            assert_eq!(int(&amounts.burn_allocation), award.burned, "{name}");
            assert_eq!(burn_weight, award.burn_weight, "{name}");
            let explicit_streams = streams
                .iter()
                .filter_map(|stream| stream.distribution.as_ref())
                .map(|distribution| {
                    let recipients = distribution.recipients.iter();
                    (
                        int(&distribution.burn_amount),
                        int(&distribution.rounding_adjustment),
                        recipients.map(|r| int(&r.earned_amount)).collect_vec(),
                    )
                })
                .collect_vec();
            assert_eq!(explicit_streams, award.explicit_streams, "{name}");
            assert_award_conserved(&streams, burn_weight, &amounts);
        }
    }

    /// 1001 attoFIL split over an implicit stream and two explicit ones, the second of which
    /// leaves a quarter of its shares unassigned.
    #[test]
    fn award_distribution_reports_streams_and_recipients_as_stored() {
        let streams = vec![
            implicit(1, D / 2),
            explicit(2, D / 100 * 29, &[(ALICE, D)], 290),
            explicit(3, D / 100 * 21, &[(BOB, D / 2), (CAROL, D / 4)], 157),
        ];
        let db = MemoryDB::default();
        let (before, after) = states(&db, streams, 1001, 447, 54);

        let (streams, _, _) = award_distribution(&before, &after, &db, 0).unwrap();

        let recipient = |recipient, share, earned| RecipientReward {
            recipient,
            share,
            earned_amount: atto(earned),
        };
        assert_eq!(
            streams,
            [
                StreamReward {
                    id: 1,
                    weight: D / 2,
                    amount: atto(500),
                    distribution: None,
                },
                StreamReward {
                    id: 2,
                    weight: D / 100 * 29,
                    amount: atto(290),
                    distribution: Some(ExplicitRewardDistribution {
                        writer: WRITER,
                        recipients: vec![recipient(ALICE, D, 290)],
                        burn_share: 0,
                        burn_amount: atto(0),
                        rounding_adjustment: atto(0),
                    }),
                },
                StreamReward {
                    id: 3,
                    weight: D / 100 * 21,
                    amount: atto(210),
                    distribution: Some(ExplicitRewardDistribution {
                        writer: WRITER,
                        recipients: vec![recipient(BOB, D / 2, 104), recipient(CAROL, D / 4, 52)],
                        burn_share: D / 4,
                        burn_amount: atto(53),
                        rounding_adjustment: atto(1),
                    }),
                },
            ]
        );
    }

    #[test]
    fn award_distribution_rejects_counters_it_cannot_reproduce() {
        let streams = || {
            vec![
                implicit(7, 3 * D / 5),
                explicit(8, 3 * D / 10, &[(ALICE, D / 2), (BOB, D / 4)], 52),
            ]
        };
        // The award allocates 22 to the explicit stream and 18 to burn.
        for (explicit, burned) in [(21, 18), (22, 17)] {
            let db = MemoryDB::default();
            let (before, after) = states(&db, streams(), 100, explicit, burned);

            let error = award_distribution(&before, &after, &db, 0).unwrap_err();

            assert_eq!(
                error.to_string(),
                "reward allocation does not match committed explicit and burn minting counters"
            );
        }
    }

    #[test]
    fn award_distribution_rejects_weights_above_the_denominator() {
        let db = MemoryDB::default();
        let streams = vec![implicit(1, D), explicit(2, 1, &[(ALICE, D)], 0)];
        let (before, after) = states(&db, streams, 0, 0, 0);

        let error = award_distribution(&before, &after, &db, 0).unwrap_err();

        assert_eq!(error.to_string(), "stream weights exceed the denominator");
    }

    /// A stream that takes one hundredth of the reward more with every epoch.
    #[test]
    fn award_distribution_evaluates_weights_at_the_award_epoch() {
        let db = MemoryDB::default();
        let ramp = WeightRecord {
            v_start: 0,
            slope: 10_000_000_000_000_000,
            t_start: 0,
            floor: 0,
            cap: D,
        };
        let stream = Stream {
            id: 1,
            weight: ramp,
            distribution: None,
        };
        let (before, after) = states(&db, vec![(stream, None)], 100, 0, 70);

        let (streams, burn_weight, amounts) = award_distribution(&before, &after, &db, 30).unwrap();

        assert_eq!(streams.first().unwrap().weight, D / 100 * 30);
        assert_eq!(burn_weight, D / 100 * 70);
        assert_eq!(amounts.miner_reward, atto(30));
    }

    /// The pool of an explicit stream is looked up only after the reward actor's own checks.
    #[test]
    fn award_distribution_refuses_a_ledger_the_reward_actor_rejects() {
        let db = MemoryDB::default();
        let (before, mut after) = states(&db, vec![explicit(2, D, &[(ALICE, D)], 0)], 0, 0, 0);
        after.accrued.clear();

        assert!(award_distribution(&before, &after, &db, 0).is_err());
    }

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

        assert_eq!(miner_paid, atto(107));
        assert_eq!(burn_paid, atto(22));
    }
}
