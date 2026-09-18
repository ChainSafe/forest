// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Reward actor migration for FIP-0118: keeps the reward accounting, drops the stored reward
//! totals and installs the bootstrap streams and the stream weight authority (SWA).
//!
//! Reference: <https://github.com/filecoin-project/go-state-types/blob/2ab83afa6e453cc38d4f3f6a496289ac0af7233e/builtin/v19/migration/reward.go>
//! and <https://github.com/filecoin-project/lotus/blob/1c89ca8f80d58717d61b6a8de460f38febe4346a/chain/consensus/filcns/upgrades.go#L3378-L3486>.

use super::reward_bootstrap::{SolsticeRewardBootstrapParams, SolsticeRewardWeightParams};
use crate::shim::address::{Address, Protocol};
use crate::shim::state_tree::StateTree;
use crate::state_migration::common::{ActorMigration, ActorMigrationInput, ActorMigrationOutput};
use crate::utils::db::CborStoreExt as _;
use anyhow::{Context as _, ensure};
use cid::Cid;
use fil_actor_reward_state::v18::State as RewardStateOld;
use fil_actor_reward_state::v19::{
    DENOM, DistributionInit, ExplicitDistribution, RecipientShare, RecipientTable,
    RegisterStreamParams, State as RewardStateNew, Stream, StreamAccrual, StreamId, StreamsState,
    WeightRecord, validate_streams_state,
};
use fil_actors_shared::v19::builtin::reward::smooth::FilterEstimate;
use fvm_ipld_blockstore::Blockstore;
use fvm_shared4::address::Address as Address_v4;
use fvm_shared4::clock::ChainEpoch;
use fvm_shared4::econ::TokenAmount;
use num_traits::Zero as _;

const CONSENSUS_STREAM_ID: StreamId = 1;
const SERVICE_STREAM_ID: StreamId = 2;

/// Consensus-only bootstrap for a network without service contracts.
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
    /// Derives and validates the bootstrap streams starting at `activation_epoch`, the first
    /// epoch executed on the migrated state.
    ///
    /// # Errors
    /// The params do not describe a valid bootstrap: a missing or non-ID address, a negative
    /// timelock or ramp, or weights out of bounds.
    pub fn new(
        params: &SolsticeRewardBootstrapParams,
        activation_epoch: ChainEpoch,
        new_code_cid: Cid,
    ) -> anyhow::Result<Self> {
        let (streams, accrued) = validate_migration_streams(
            &bootstrap_streams(params, activation_epoch)?,
            activation_epoch,
        )?;
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

    /// Checks the SWA, every distribution writer and every share recipient against `actors`.
    ///
    /// # Errors
    /// `paych_code` is `None`, or a referenced actor is the burn actor, is missing, or is a
    /// payment channel.
    pub fn validate_recipients<BS: Blockstore>(
        &self,
        actors: &StateTree<BS>,
        paych_code: Option<Cid>,
    ) -> anyhow::Result<()> {
        let paych_code =
            paych_code.context("code cid for payment channel actor not found in old manifest")?;
        validate_actor_reference(actors, self.swa_actor, "SWA actor", paych_code)?;
        for stream in &self.streams.streams {
            let Some(distribution) = &stream.distribution else {
                continue;
            };
            validate_actor_reference(
                actors,
                distribution.writer,
                "distribution writer",
                paych_code,
            )?;
            for share in &distribution.shares {
                validate_actor_reference(actors, share.recipient, "reward recipient", paych_code)?;
            }
        }
        Ok(())
    }
}

/// Rejects an actor the reward state must not name: the burn actor, one missing from `actors`,
/// or a payment channel, which `Collect` deletes and would strand its unpaid rewards.
fn validate_actor_reference<BS: Blockstore>(
    actors: &StateTree<BS>,
    address: Address_v4,
    label: &str,
    paych_code: Cid,
) -> anyhow::Result<()> {
    let address = Address::from(address);
    ensure!(
        address != Address::BURNT_FUNDS_ACTOR,
        "{label} is the burn actor"
    );
    let actor = actors
        .get_actor(&address)
        .with_context(|| format!("failed to load {label} {address}"))?
        .with_context(|| format!("{label} {address} does not exist"))?;
    ensure!(
        actor.code != paych_code,
        "{label} {address} is a payment channel"
    );
    Ok(())
}

/// The streams to register: consensus alone at constant `DENOM` for a zero ramp, otherwise
/// consensus and service trading weight at the same rate.
fn bootstrap_streams(
    params: &SolsticeRewardBootstrapParams,
    activation_epoch: ChainEpoch,
) -> anyhow::Result<Vec<RegisterStreamParams>> {
    let record = |weight: SolsticeRewardWeightParams, slope: i64| WeightRecord {
        v_start: weight.v_start,
        slope,
        t_start: activation_epoch,
        floor: weight.floor,
        cap: weight.cap,
    };

    if params.consensus_weight_ramp_duration_epochs == 0 {
        ensure!(
            params.consensus_weight == NEUTRAL_CONSENSUS_WEIGHT
                && params.service_weight == NO_SERVICE_WEIGHT,
            "zero-duration Solstice bootstrap must have constant DENOM consensus weight and zero service weight"
        );
        return Ok(vec![RegisterStreamParams {
            id: CONSENSUS_STREAM_ID,
            weight: record(params.consensus_weight, 0),
            distribution: None,
            activation_epoch,
        }]);
    }

    let slope = consensus_weight_slope(
        params.consensus_weight,
        params.consensus_weight_ramp_duration_epochs,
    )?;
    let sra_actor = required_address(params.sra_actor, "SRA actor")?;
    let initial_orchestrator =
        required_address(params.initial_orchestrator, "initial orchestrator")?;
    Ok(vec![
        RegisterStreamParams {
            id: CONSENSUS_STREAM_ID,
            weight: record(params.consensus_weight, -slope),
            distribution: None,
            activation_epoch,
        },
        RegisterStreamParams {
            id: SERVICE_STREAM_ID,
            weight: record(params.service_weight, slope),
            distribution: Some(DistributionInit {
                writer: sra_actor,
                shares: vec![RecipientShare {
                    recipient: initial_orchestrator,
                    share: DENOM,
                }],
            }),
            activation_epoch,
        },
    ])
}

/// Builds the streams a network upgrade installs and validates them with the actor crate:
/// stream 1 alone at constant `DENOM`, or streams 1 and 2 with equal and opposite slopes,
/// starting weights summing to `DENOM` and one full-share recipient.
fn validate_migration_streams(
    params: &[RegisterStreamParams],
    activation_epoch: ChainEpoch,
) -> anyhow::Result<(StreamsState, Vec<StreamAccrual>)> {
    ensure!(
        params.len() == 1 || params.len() == 2,
        "bootstrap requires one or two streams"
    );
    for param in params {
        ensure!(
            param.activation_epoch == activation_epoch,
            "stream {} activation epoch {} does not match upgrade epoch {activation_epoch}",
            param.id,
            param.activation_epoch
        );
        ensure!(
            param.weight.t_start == activation_epoch,
            "stream {} weight start {} does not match upgrade epoch {activation_epoch}",
            param.id,
            param.weight.t_start
        );
    }

    if let [consensus] = params {
        let neutral = WeightRecord {
            v_start: DENOM,
            slope: 0,
            t_start: activation_epoch,
            floor: DENOM,
            cap: DENOM,
        };
        ensure!(
            consensus.id == 1 && consensus.distribution.is_none() && consensus.weight == neutral,
            "single-stream bootstrap must be implicit stream 1 at constant DENOM"
        );
    } else if let [consensus, explicit] = params {
        ensure!(
            consensus.id == 1 && explicit.id == 2,
            "split bootstrap stream IDs must be 1 and 2"
        );
        let distribution = match (&consensus.distribution, &explicit.distribution) {
            (None, Some(distribution)) => distribution,
            _ => anyhow::bail!("split bootstrap distribution forms are invalid"),
        };
        ensure!(
            consensus.weight.v_start <= DENOM
                && explicit.weight.v_start == DENOM - consensus.weight.v_start,
            "bootstrap starting weights must sum to denominator"
        );
        ensure!(
            consensus.weight.slope < 0
                && explicit.weight.slope > 0
                && consensus.weight.slope == -explicit.weight.slope,
            "bootstrap weight slopes are invalid"
        );
        ensure!(
            matches!(distribution.shares.as_slice(), [share] if share.share == DENOM),
            "explicit bootstrap requires one full-share recipient"
        );
    }

    let mut streams = StreamsState::default();
    let mut accrued = Vec::new();
    for param in params {
        let distribution = param
            .distribution
            .as_ref()
            .map(|init| ExplicitDistribution {
                writer: init.writer,
                shares: init.shares.clone(),
                payable: RecipientTable::default(),
                claimed_period: RecipientTable::default(),
            });
        if distribution.is_some() {
            accrued.push(StreamAccrual {
                id: param.id,
                amount: TokenAmount::zero(),
            });
        }
        streams.streams.push(Stream {
            id: param.id,
            weight: param.weight.clone(),
            distribution,
        });
    }
    validate_streams_state(&streams, &accrued, activation_epoch)?;
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
    use crate::shim::state_tree::{ActorState, StateTreeVersion};
    use crate::utils::cid::CidCborExt as _;
    use fil_actors_shared::v18::builtin::reward::smooth::FilterEstimate as FilterEstimateOld;
    use std::sync::Arc;

    use super::super::reward_bootstrap::PERCENT;

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
            // 45% of the reward moves over 45 epochs: one percent per epoch.
            consensus_weight_ramp_duration_epochs: 45,
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
                pending_writes_queue: vec![],
            }
        );
        assert!(migrator.accrued.is_empty());
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

    #[test]
    fn rejects_malformed_bootstrap_streams() {
        type Damage = fn(&mut Vec<RegisterStreamParams>);
        let cases: [(&str, Damage, &str); 11] = [
            (
                "three streams",
                |p| p.push(p.last().cloned().unwrap()),
                "bootstrap requires one or two streams",
            ),
            (
                "activation epoch mismatch",
                |p| p.first_mut().unwrap().activation_epoch += 1,
                "activation epoch 101 does not match upgrade epoch 100",
            ),
            (
                "weight start mismatch",
                |p| p.first_mut().unwrap().weight.t_start += 1,
                "weight start 101 does not match upgrade epoch 100",
            ),
            (
                "single stream that is not neutral",
                |p| p.truncate(1),
                "single-stream bootstrap must be implicit stream 1 at constant DENOM",
            ),
            (
                "service stream ID 3",
                |p| p.last_mut().unwrap().id = 3,
                "split bootstrap stream IDs must be 1 and 2",
            ),
            (
                "two implicit streams",
                |p| p.last_mut().unwrap().distribution = None,
                "split bootstrap distribution forms are invalid",
            ),
            (
                "starting weights under-sum",
                |p| p.last_mut().unwrap().weight.v_start -= 1,
                "bootstrap starting weights must sum to denominator",
            ),
            (
                "unequal slopes",
                |p| p.last_mut().unwrap().weight.slope += 1,
                "bootstrap weight slopes are invalid",
            ),
            (
                "partial share",
                |p| {
                    let distribution = p.last_mut().unwrap().distribution.as_mut().unwrap();
                    distribution.shares.first_mut().unwrap().share -= 1;
                },
                "explicit bootstrap requires one full-share recipient",
            ),
            (
                "delegated writer",
                |p| {
                    let distribution = p.last_mut().unwrap().distribution.as_mut().unwrap();
                    distribution.writer = Address_v4::new_delegated(10, &[1]).unwrap();
                },
                "distribution writer f410",
            ),
            (
                "service cap above what the consensus floor leaves",
                |p| p.last_mut().unwrap().weight.cap = 60 * PERCENT,
                "stream weights exceed DENOM",
            ),
        ];

        for (case, damage, expected_error) in cases {
            let mut params = bootstrap_streams(&bootstrap_params(), 100).unwrap();
            damage(&mut params);
            let error = validate_migration_streams(&params, 100)
                .err()
                .unwrap_or_else(|| panic!("{case}: accepted"));
            assert!(
                format!("{error:#}").contains(expected_error),
                "{case}: {error:#}"
            );
        }
    }

    #[test]
    fn scheduled_networks_have_complete_bootstrap_addresses() {
        for config in [
            ChainConfig::mainnet(),
            ChainConfig::calibnet(),
            ChainConfig::butterflynet(),
        ] {
            let solstice_epoch = config.epoch(Height::Solstice);
            if solstice_epoch == UPGRADE_HEIGHT_UNSCHEDULED {
                continue;
            }
            let params = SolsticeRewardBootstrapParams::for_chain(&config.network);
            assert!(
                params.swa_actor.is_some()
                    && params.sra_actor.is_some()
                    && params.initial_orchestrator.is_some(),
                "{}: scheduled without all bootstrap addresses",
                config.network
            );
            // The migration resolves the addresses on chain; stand-ins leave the weights to check.
            let params = SolsticeRewardBootstrapParams {
                swa_actor: Some(Address::new_id(100)),
                sra_actor: Some(Address::new_id(101)),
                initial_orchestrator: Some(Address::new_id(102)),
                ..params
            };
            RewardMigrator::new(&params, solstice_epoch + 1, Cid::default()).unwrap_or_else(|e| {
                panic!(
                    "{}: scheduled without a valid bootstrap: {e:#}",
                    config.network
                )
            });
        }
    }

    #[test]
    fn validates_every_actor_the_bootstrap_references() {
        let account = Cid::from_cbor_blake2b256(&"account code").unwrap();
        let paych = Cid::from_cbor_blake2b256(&"paych code").unwrap();
        // The SWA, SRA and orchestrator of `bootstrap_params`.
        let (swa, sra, orchestrator) = (
            Address::new_id(100),
            Address::new_id(101),
            Address::new_id(102),
        );
        let burn = Some(Address::BURNT_FUNDS_ACTOR);
        let system = Some(Address::SYSTEM_ACTOR);

        // On-chain actors: all three as accounts, minus `missing`, with `channel` as a paych.
        let on_chain =
            |missing: Option<Address>, channel: Option<Address>| -> Vec<(Address, Cid)> {
                [swa, sra, orchestrator]
                    .into_iter()
                    .filter(|address| Some(*address) != missing)
                    .map(|address| {
                        let code = if Some(address) == channel {
                            paych
                        } else {
                            account
                        };
                        (address, code)
                    })
                    .collect()
            };
        let split = bootstrap_params();
        let neutral = SolsticeRewardBootstrapParams {
            consensus_weight_ramp_duration_epochs: 0,
            consensus_weight: NEUTRAL_CONSENSUS_WEIGHT,
            service_weight: NO_SERVICE_WEIGHT,
            ..bootstrap_params()
        };

        for (case, params, actors, paych_code, expected) in [
            (
                "account references",
                split.clone(),
                on_chain(None, None),
                Some(paych),
                Ok(()),
            ),
            (
                "system actor references",
                SolsticeRewardBootstrapParams {
                    swa_actor: system,
                    sra_actor: system,
                    ..split.clone()
                },
                vec![(Address::SYSTEM_ACTOR, account), (orchestrator, account)],
                Some(paych),
                Ok(()),
            ),
            (
                "neutral bootstrap needs only its SWA",
                neutral.clone(),
                vec![(swa, account)],
                Some(paych),
                Ok(()),
            ),
            (
                "payment channel SWA",
                split.clone(),
                on_chain(None, Some(swa)),
                Some(paych),
                Err("SWA actor f0100 is a payment channel"),
            ),
            (
                "payment channel distribution writer",
                split.clone(),
                on_chain(None, Some(sra)),
                Some(paych),
                Err("distribution writer f0101 is a payment channel"),
            ),
            (
                "payment channel recipient",
                split.clone(),
                on_chain(None, Some(orchestrator)),
                Some(paych),
                Err("reward recipient f0102 is a payment channel"),
            ),
            (
                "missing SWA",
                split.clone(),
                on_chain(Some(swa), None),
                Some(paych),
                Err("SWA actor f0100 does not exist"),
            ),
            (
                "missing SWA of a neutral bootstrap",
                neutral,
                vec![],
                Some(paych),
                Err("SWA actor f0100 does not exist"),
            ),
            (
                "missing distribution writer",
                split.clone(),
                on_chain(Some(sra), None),
                Some(paych),
                Err("distribution writer f0101 does not exist"),
            ),
            (
                "missing recipient",
                split.clone(),
                on_chain(Some(orchestrator), None),
                Some(paych),
                Err("reward recipient f0102 does not exist"),
            ),
            (
                "burn SWA",
                SolsticeRewardBootstrapParams {
                    swa_actor: burn,
                    ..split.clone()
                },
                on_chain(None, None),
                Some(paych),
                Err("SWA actor is the burn actor"),
            ),
            (
                "burn distribution writer",
                SolsticeRewardBootstrapParams {
                    sra_actor: burn,
                    ..split.clone()
                },
                on_chain(None, None),
                Some(paych),
                Err("distribution writer is the burn actor"),
            ),
            (
                "no payment channel code in the old manifest",
                split,
                on_chain(None, None),
                None,
                Err("code cid for payment channel actor not found in old manifest"),
            ),
        ] {
            let mut tree =
                StateTree::new(&Arc::new(MemoryDB::default()), StateTreeVersion::V5).unwrap();
            for (address, code) in actors {
                let actor =
                    ActorState::new(code, Cid::default(), TokenAmount::zero().into(), 0, None);
                tree.set_actor(&address, actor).unwrap();
            }
            let migrator = RewardMigrator::new(&params, 100, Cid::default())
                .unwrap_or_else(|e| panic!("{case}: {e:#}"));

            let result = migrator.validate_recipients(&tree, paych_code);

            match expected {
                Ok(()) => result.unwrap_or_else(|e| panic!("{case}: {e:#}")),
                Err(message) => {
                    let error = result
                        .err()
                        .unwrap_or_else(|| panic!("{case}: accepted"))
                        .to_string();
                    assert!(error.contains(message), "{case}: {error}");
                }
            }
        }
    }
}
