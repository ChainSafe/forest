// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Reward actor migration for FIP-0118: keeps the reward accounting, drops the stored reward
//! totals and installs the bootstrap streams and the stream weight authority (SWA).
//!
//! Ports the go-state-types migrator and the Lotus stream derivation; the built streams are
//! vetted by the actor crate's own `validate_streams_state`, the check the actor repeats on
//! every block reward:
//! <https://github.com/filecoin-project/go-state-types/blob/6cb27cf2e8be76d9b20f0d58d6d580cd99e31ce6/builtin/v19/migration/reward.go>
//! <https://github.com/filecoin-project/lotus/blob/1b0155685292f691babd930f1060562ecff645c3/chain/consensus/filcns/upgrades.go#L3363-L3433>

use crate::networks::{SolsticeRewardBootstrapParams, SolsticeRewardWeightParams};
use crate::shim::address::{Address, Protocol};
use crate::state_migration::common::{ActorMigration, ActorMigrationInput, ActorMigrationOutput};
use crate::utils::db::CborStoreExt as _;
use anyhow::{Context as _, ensure};
use cid::Cid;
use fil_actor_reward_state::v18::State as RewardStateOld;
use fil_actor_reward_state::v19::{
    DENOM, ExplicitDistribution, RecipientShare, State as RewardStateNew, Stream, StreamAccrual,
    StreamId, StreamsState, WeightRecord, validate_streams_state,
};
use fil_actors_shared::v19::builtin::reward::smooth::FilterEstimate;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared4::address::Address as Address_v4;
use fvm_shared4::clock::ChainEpoch;
use fvm_shared4::econ::TokenAmount;
use num_traits::Zero as _;

const CONSENSUS_STREAM_ID: StreamId = 1;
const SERVICE_STREAM_ID: StreamId = 2;

/// Consensus-only bootstrap: the whole block reward keeps flowing to block producers.
const NEUTRAL_CONSENSUS_WEIGHT: SolsticeRewardWeightParams = SolsticeRewardWeightParams {
    v_start: DENOM,
    floor: DENOM,
    cap: DENOM,
};
const NO_SERVICE_WEIGHT: SolsticeRewardWeightParams = SolsticeRewardWeightParams {
    v_start: 0,
    floor: 0,
    cap: 0,
};

pub struct RewardMigrator {
    new_code_cid: Cid,
    streams: StreamsState,
    accrued: Vec<StreamAccrual>,
    swa_timelock_epochs: ChainEpoch,
    swa_actor: Address_v4,
}

impl RewardMigrator {
    /// Derives the bootstrap streams starting at `activation_epoch`, the first epoch executed on
    /// the migrated state, and vets them with the actor's own state validation.
    ///
    /// # Errors
    /// Fails on the inputs Lotus and go-state-types reject: a missing or non-ID bootstrap
    /// address, a negative SWA timelock, a negative ramp, a zero ramp with anything but the
    /// neutral weights, or weights that do not start at `DENOM` together, leave their bounds or
    /// exceed `DENOM` later.
    pub fn new(
        params: &SolsticeRewardBootstrapParams,
        activation_epoch: ChainEpoch,
        new_code_cid: Cid,
    ) -> anyhow::Result<Self> {
        let (streams, accrued) = bootstrap_streams(params, activation_epoch)?;
        validate_streams_state(&streams, &accrued, activation_epoch)?;
        ensure!(params.swa_timelock_epochs >= 0, "SWA timelock is negative");
        let swa_actor = required_address(params.swa_actor, "SWA actor")?;
        ensure!(
            swa_actor.protocol() == Protocol::ID,
            "SWA actor is not an ID address"
        );

        Ok(Self {
            new_code_cid,
            streams,
            accrued,
            swa_timelock_epochs: params.swa_timelock_epochs,
            swa_actor,
        })
    }
}

/// Bootstrap streams and their accruals as Lotus derives them: the consensus stream alone at
/// constant `DENOM` for a zero ramp, otherwise consensus and service streams trading weight at
/// the same rate until the consensus stream reaches its floor, with one zero accrual for the
/// service stream.
fn bootstrap_streams(
    params: &SolsticeRewardBootstrapParams,
    activation_epoch: ChainEpoch,
) -> anyhow::Result<(StreamsState, Vec<StreamAccrual>)> {
    let record = |weight: SolsticeRewardWeightParams, slope: i64| WeightRecord {
        v_start: weight.v_start,
        slope,
        t_start: activation_epoch,
        floor: weight.floor,
        cap: weight.cap,
    };

    let streams = if params.consensus_weight_ramp_duration_epochs == 0 {
        ensure!(
            params.consensus_weight == NEUTRAL_CONSENSUS_WEIGHT
                && params.service_weight == NO_SERVICE_WEIGHT,
            "zero-duration Solstice bootstrap must have constant DENOM consensus weight and zero service weight"
        );
        vec![Stream {
            id: CONSENSUS_STREAM_ID,
            weight: record(params.consensus_weight, 0),
            distribution: None,
        }]
    } else {
        let slope = consensus_weight_slope(
            params.consensus_weight,
            params.consensus_weight_ramp_duration_epochs,
        )?;
        // The actor accepts weights that start below `DENOM` (the rest burns); go-state-types
        // does not, so Lotus would refuse such a bootstrap.
        ensure!(
            params.consensus_weight.v_start <= DENOM
                && params.service_weight.v_start == DENOM - params.consensus_weight.v_start,
            "bootstrap starting weights must sum to denominator"
        );
        let sra_actor = required_address(params.sra_actor, "SRA actor")?;
        let initial_orchestrator =
            required_address(params.initial_orchestrator, "initial orchestrator")?;
        vec![
            Stream {
                id: CONSENSUS_STREAM_ID,
                weight: record(params.consensus_weight, -slope),
                distribution: None,
            },
            Stream {
                id: SERVICE_STREAM_ID,
                weight: record(params.service_weight, slope),
                distribution: Some(ExplicitDistribution {
                    writer: sra_actor,
                    shares: vec![RecipientShare {
                        recipient: initial_orchestrator,
                        share: DENOM,
                    }],
                    payable: Vec::new(),
                    claimed_period: Vec::new(),
                }),
            },
        ]
    };
    let accrued = streams
        .iter()
        .filter(|stream| stream.distribution.is_some())
        .map(|stream| StreamAccrual {
            id: stream.id,
            amount: TokenAmount::zero(),
        })
        .collect();
    let streams = StreamsState {
        streams,
        tombstones: Vec::new(),
        pending_writes: Vec::new(),
    };
    Ok((streams, accrued))
}

/// Weight moved from the consensus stream to the service stream each epoch, rounded up so the
/// consensus weight reaches its floor within the ramp even when the total is not divisible.
fn consensus_weight_slope(
    weight: SolsticeRewardWeightParams,
    ramp_epochs: ChainEpoch,
) -> anyhow::Result<i64> {
    ensure!(
        ramp_epochs > 0,
        "Solstice consensus weight ramp duration is negative: {ramp_epochs}"
    );
    ensure!(
        weight.v_start > weight.floor,
        "Solstice consensus weight start {} must exceed its floor {}",
        weight.v_start,
        weight.floor
    );
    let slope = (weight.v_start - weight.floor).div_ceil(ramp_epochs.unsigned_abs());
    i64::try_from(slope)
        .with_context(|| format!("Solstice consensus weight ramp produces invalid slope {slope}"))
}

/// Lotus passes an unset address through as `address.Undef` and lets the ID check reject it;
/// Forest models unset as `None` and names the missing input.
fn required_address(address: Option<Address>, name: &str) -> anyhow::Result<Address_v4> {
    let address = address.with_context(|| {
        format!("{name} is not set: the Solstice migration needs its f0 address")
    })?;
    Ok(Address_v4::from(&address))
}

impl<BS: Blockstore> ActorMigration<BS> for RewardMigrator {
    fn migrate_state(
        &self,
        store: &BS,
        input: ActorMigrationInput,
    ) -> anyhow::Result<Option<ActorMigrationOutput>> {
        let in_state: RewardStateOld = store.get_cbor_required(&input.head)?;
        let streams_root = store.put_cbor_default(&self.streams)?;
        // `simple_total` and `baseline_total` are dropped: v19 derives them from constants.
        let out_state = RewardStateNew {
            cumsum_baseline: in_state.cumsum_baseline,
            cumsum_realized: in_state.cumsum_realized,
            effective_network_time: in_state.effective_network_time,
            effective_baseline_power: in_state.effective_baseline_power,
            this_epoch_reward: in_state.this_epoch_reward,
            this_epoch_reward_smoothed: FilterEstimate {
                position: in_state.this_epoch_reward_smoothed.position,
                velocity: in_state.this_epoch_reward_smoothed.velocity,
            },
            this_epoch_baseline_power: in_state.this_epoch_baseline_power,
            epoch: in_state.epoch,
            total_minted_reward: in_state.total_storage_power_reward,
            total_burn_minted: TokenAmount::zero(),
            total_explicit_minted: TokenAmount::zero(),
            accrued: self.accrued.clone(),
            swa_timelock_epochs: self.swa_timelock_epochs,
            swa_actor: self.swa_actor,
            streams_root,
        };
        let new_head = store.put_cbor_default(&out_state)?;
        Ok(Some(ActorMigrationOutput {
            new_code_cid: self.new_code_cid,
            new_head,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::MemoryDB;
    use crate::networks::{ChainConfig, Height, UPGRADE_HEIGHT_UNSCHEDULED};
    use crate::utils::cid::CidCborExt as _;
    use fil_actors_shared::v18::builtin::reward::smooth::FilterEstimate as FilterEstimateOld;

    const PERCENT: u64 = DENOM / 100;

    fn weight(v_start: u64, floor: u64, cap: u64) -> SolsticeRewardWeightParams {
        SolsticeRewardWeightParams {
            v_start: v_start * PERCENT,
            floor: floor * PERCENT,
            cap: cap * PERCENT,
        }
    }

    fn bootstrap_params() -> SolsticeRewardBootstrapParams {
        SolsticeRewardBootstrapParams {
            swa_timelock_epochs: 20_160,
            consensus_weight_ramp_duration_epochs: 81,
            consensus_weight: weight(95, 50, 95),
            service_weight: weight(5, 5, 10),
            swa_actor: Some(Address::new_id(100)),
            sra_actor: Some(Address::new_id(101)),
            initial_orchestrator: Some(Address::new_id(102)),
        }
    }

    #[test]
    fn migrates_v18_state_and_installs_bootstrap_streams() {
        let store = MemoryDB::default();
        // Distinct values in every field, so a shifted field would show up as a wrong value.
        let in_state = RewardStateOld {
            cumsum_baseline: 1.into(),
            cumsum_realized: 2.into(),
            effective_network_time: 3,
            effective_baseline_power: 4.into(),
            this_epoch_reward: TokenAmount::from_atto(5),
            this_epoch_reward_smoothed: FilterEstimateOld {
                position: 6.into(),
                velocity: 7.into(),
            },
            this_epoch_baseline_power: 8.into(),
            epoch: 9,
            total_storage_power_reward: TokenAmount::from_atto(10),
            simple_total: TokenAmount::from_atto(11),
            baseline_total: TokenAmount::from_atto(12),
        };
        let head = store.put_cbor_default(&in_state).unwrap();
        let new_code_cid = Cid::from_cbor_blake2b256(&"reward v19 code").unwrap();
        let activation_epoch = 100;

        let output = RewardMigrator::new(&bootstrap_params(), activation_epoch, new_code_cid)
            .unwrap()
            .migrate_state(&store, ActorMigrationInput::for_head(head))
            .unwrap()
            .unwrap();
        assert_eq!(output.new_code_cid, new_code_cid);

        // 45% of DENOM moves from consensus to service over the 81-epoch ramp, rounded up.
        let slope = 5_555_555_555_555_556;
        let expected_streams = StreamsState {
            streams: vec![
                Stream {
                    id: 1,
                    weight: WeightRecord {
                        v_start: 95 * PERCENT,
                        slope: -slope,
                        t_start: activation_epoch,
                        floor: 50 * PERCENT,
                        cap: 95 * PERCENT,
                    },
                    distribution: None,
                },
                Stream {
                    id: 2,
                    weight: WeightRecord {
                        v_start: 5 * PERCENT,
                        slope,
                        t_start: activation_epoch,
                        floor: 5 * PERCENT,
                        cap: 10 * PERCENT,
                    },
                    distribution: Some(ExplicitDistribution {
                        writer: Address_v4::new_id(101),
                        shares: vec![RecipientShare {
                            recipient: Address_v4::new_id(102),
                            share: DENOM,
                        }],
                        payable: vec![],
                        claimed_period: vec![],
                    }),
                },
            ],
            tombstones: vec![],
            pending_writes: vec![],
        };
        let out_state: RewardStateNew = store.get_cbor_required(&output.new_head).unwrap();
        assert_eq!(
            store
                .get_cbor_required::<StreamsState>(&out_state.streams_root)
                .unwrap(),
            expected_streams
        );

        let expected = RewardStateNew {
            cumsum_baseline: 1.into(),
            cumsum_realized: 2.into(),
            effective_network_time: 3,
            effective_baseline_power: 4.into(),
            this_epoch_reward: TokenAmount::from_atto(5),
            this_epoch_reward_smoothed: FilterEstimate {
                position: 6.into(),
                velocity: 7.into(),
            },
            this_epoch_baseline_power: 8.into(),
            epoch: 9,
            total_minted_reward: TokenAmount::from_atto(10),
            total_burn_minted: TokenAmount::zero(),
            total_explicit_minted: TokenAmount::zero(),
            accrued: vec![StreamAccrual {
                id: 2,
                amount: TokenAmount::zero(),
            }],
            swa_timelock_epochs: 20_160,
            swa_actor: Address_v4::new_id(100),
            streams_root: store.put_cbor_default(&expected_streams).unwrap(),
        };
        // `State` has no `PartialEq`.
        assert_eq!(format!("{out_state:?}"), format!("{expected:?}"));
    }

    #[test]
    fn zero_ramp_installs_the_consensus_stream_alone() {
        let params = SolsticeRewardBootstrapParams {
            consensus_weight_ramp_duration_epochs: 0,
            consensus_weight: weight(100, 100, 100),
            service_weight: weight(0, 0, 0),
            sra_actor: None,
            initial_orchestrator: None,
            ..bootstrap_params()
        };

        let migrator = RewardMigrator::new(&params, 100, Cid::default()).unwrap();

        assert_eq!(
            migrator.streams,
            StreamsState {
                streams: vec![Stream {
                    id: 1,
                    weight: WeightRecord {
                        v_start: DENOM,
                        slope: 0,
                        t_start: 100,
                        floor: DENOM,
                        cap: DENOM,
                    },
                    distribution: None,
                }],
                tombstones: vec![],
                pending_writes: vec![],
            }
        );
        assert!(migrator.accrued.is_empty());
    }

    #[test]
    fn consensus_weight_slope_rounds_up_to_reach_the_floor_within_the_ramp() {
        // (ramp epochs, per-epoch slope): 45% of DENOM spread over the ramp.
        for (ramp_epochs, expected_slope) in [
            (900, 500_000_000_000_000),
            (81, 5_555_555_555_555_556),
            (20_160, 22_321_428_571_429),
            (2_332_800, 192_901_234_568),
        ] {
            assert_eq!(
                consensus_weight_slope(weight(95, 50, 95), ramp_epochs).unwrap(),
                expected_slope
            );
        }
    }

    #[test]
    fn rejects_incomplete_or_invalid_bootstrap_params() {
        let valid = bootstrap_params();
        let delegated = Some(Address::new_delegated(10, &[1]).unwrap());
        for (case, params, expected_error) in [
            (
                "unset SWA",
                SolsticeRewardBootstrapParams {
                    swa_actor: None,
                    ..valid.clone()
                },
                "SWA actor is not set",
            ),
            (
                "unset SRA",
                SolsticeRewardBootstrapParams {
                    sra_actor: None,
                    ..valid.clone()
                },
                "SRA actor is not set",
            ),
            (
                "unset orchestrator",
                SolsticeRewardBootstrapParams {
                    initial_orchestrator: None,
                    ..valid.clone()
                },
                "initial orchestrator is not set",
            ),
            (
                "non-ID SWA",
                SolsticeRewardBootstrapParams {
                    swa_actor: delegated,
                    ..valid.clone()
                },
                "SWA actor is not an ID address",
            ),
            (
                "non-ID SRA",
                SolsticeRewardBootstrapParams {
                    sra_actor: delegated,
                    ..valid.clone()
                },
                "distribution writer f410",
            ),
            (
                "non-ID orchestrator",
                SolsticeRewardBootstrapParams {
                    initial_orchestrator: delegated,
                    ..valid.clone()
                },
                "share recipient f410",
            ),
            (
                "negative timelock",
                SolsticeRewardBootstrapParams {
                    swa_timelock_epochs: -1,
                    ..valid.clone()
                },
                "SWA timelock is negative",
            ),
            (
                "negative ramp",
                SolsticeRewardBootstrapParams {
                    consensus_weight_ramp_duration_epochs: -1,
                    ..valid.clone()
                },
                "ramp duration is negative",
            ),
            (
                "zero ramp with split weights",
                SolsticeRewardBootstrapParams {
                    consensus_weight_ramp_duration_epochs: 0,
                    ..valid.clone()
                },
                "zero-duration Solstice bootstrap must have constant DENOM consensus weight and zero service weight",
            ),
            (
                "consensus start not above its floor",
                SolsticeRewardBootstrapParams {
                    consensus_weight: weight(50, 50, 95),
                    ..valid.clone()
                },
                "must exceed its floor",
            ),
            (
                "starting weights do not sum to DENOM",
                SolsticeRewardBootstrapParams {
                    service_weight: weight(6, 5, 10),
                    ..valid.clone()
                },
                "starting weights must sum to denominator",
            ),
            (
                "service cap above what the consensus floor leaves",
                SolsticeRewardBootstrapParams {
                    service_weight: weight(5, 5, 60),
                    ..valid.clone()
                },
                "stream weights exceed DENOM",
            ),
            (
                "weight start above its cap",
                SolsticeRewardBootstrapParams {
                    consensus_weight: weight(95, 50, 94),
                    ..valid
                },
                "weight v_start exceeds cap",
            ),
        ] {
            let error = RewardMigrator::new(&params, 100, Cid::default())
                .err()
                .unwrap_or_else(|| panic!("{case}: accepted"));
            assert!(
                format!("{error:#}").contains(expected_error),
                "{case}: {error:#}"
            );
        }
    }

    // The bootstrap names contracts that must exist before the upgrade runs, so a scheduled
    // height and complete addresses go together.
    #[test]
    fn scheduled_networks_have_complete_bootstrap_addresses() {
        for config in [
            ChainConfig::mainnet(),
            ChainConfig::calibnet(),
            ChainConfig::butterflynet(),
        ] {
            let solstice_epoch = config.epoch(Height::Solstice);
            let scheduled = solstice_epoch != UPGRADE_HEIGHT_UNSCHEDULED;
            let bootstrap = RewardMigrator::new(
                &config.solstice_reward_bootstrap,
                solstice_epoch + 1,
                Cid::default(),
            );
            assert_eq!(
                bootstrap.is_ok(),
                scheduled,
                "{}: schedule Solstice only once SWA, SRA and orchestrator have f0 addresses",
                config.network
            );
        }
    }

    // Lotus 2k names the burnt-funds actor as orchestrator. The reward actor rejects that as
    // stored state and pays no block reward on it, while go-state-types accepts it; Forest
    // follows the actor. Re-sync the devnet params once upstream agrees.
    #[test]
    fn devnet_bootstrap_is_rejected_until_upstream_agrees_on_the_orchestrator() {
        let error = RewardMigrator::new(
            &ChainConfig::devnet().solstice_reward_bootstrap,
            1,
            Cid::default(),
        )
        .err()
        .expect("burnt-funds orchestrator accepted");
        assert!(
            format!("{error:#}").contains("burn sentinel persisted as a recipient"),
            "{error:#}"
        );
    }

    // Only the addresses are missing on the public networks; their timelocks, ramps and weights
    // already pass the checks.
    #[test]
    fn public_network_params_are_valid_once_addresses_are_set() {
        for config in [
            ChainConfig::mainnet(),
            ChainConfig::calibnet(),
            ChainConfig::butterflynet(),
        ] {
            let params = SolsticeRewardBootstrapParams {
                swa_actor: Some(Address::new_id(100)),
                sra_actor: Some(Address::new_id(101)),
                initial_orchestrator: Some(Address::new_id(102)),
                ..config.solstice_reward_bootstrap
            };
            RewardMigrator::new(&params, 1, Cid::default())
                .unwrap_or_else(|e| panic!("{}: {e:#}", config.network));
        }
    }
}
