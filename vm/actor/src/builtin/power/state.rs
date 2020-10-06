// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{CONSENSUS_MINER_MIN_MINERS, CONSENSUS_MINER_MIN_POWER};
use crate::{
    make_map_with_root,
    smooth::{AlphaBetaFilter, FilterEstimate, DEFAULT_ALPHA, DEFAULT_BETA},
    ActorDowncast, BytesKey, Map, Multimap,
};
use address::Address;
use cid::Cid;
use clock::{ChainEpoch, EPOCH_UNDEFINED};
use encoding::{tuple::*, Cbor};
use fil_types::StoragePower;
use integer_encoding::VarInt;
use ipld_blockstore::BlockStore;
use num_bigint::{bigint_ser, BigInt, Sign};
use std::error::Error as StdError;
use vm::{actor_error, ActorError, ExitCode, Serialized, TokenAmount};

lazy_static! {
    /// genesis power in bytes = 750,000 GiB
    static ref INITIAL_QA_POWER_ESTIMATE_POSITION: BigInt = BigInt::from(750_000) * (1 << 30);
    /// max chain throughput in bytes per epoch = 120 ProveCommits / epoch = 3,840 GiB
    static ref INITIAL_QA_POWER_ESTIMATE_VELOCITY: BigInt = BigInt::from(3_840) * (1 << 30);
}

/// Storage power actor state
#[derive(Default, Serialize_tuple, Deserialize_tuple)]
pub struct State {
    #[serde(with = "bigint_ser")]
    pub total_raw_byte_power: StoragePower,
    #[serde(with = "bigint_ser")]
    pub total_bytes_committed: StoragePower,
    #[serde(with = "bigint_ser")]
    pub total_quality_adj_power: StoragePower,
    #[serde(with = "bigint_ser")]
    pub total_qa_bytes_committed: StoragePower,
    #[serde(with = "bigint_ser")]
    pub total_pledge_collateral: TokenAmount,

    #[serde(with = "bigint_ser")]
    pub this_epoch_raw_byte_power: StoragePower,
    #[serde(with = "bigint_ser")]
    pub this_epoch_quality_adj_power: StoragePower,
    #[serde(with = "bigint_ser")]
    pub this_epoch_pledge_collateral: TokenAmount,
    pub this_epoch_qa_power_smoothed: FilterEstimate,

    pub miner_count: i64,
    /// Number of miners having proven the minimum consensus power.
    pub miner_above_min_power_count: i64,

    /// A queue of events to be triggered by cron, indexed by epoch.
    pub cron_event_queue: Cid, // Multimap, (HAMT[ChainEpoch]AMT[CronEvent]

    /// First epoch in which a cron task may be stored. Cron will iterate every epoch between this
    /// and the current epoch inclusively to find tasks to execute.
    pub first_cron_epoch: ChainEpoch,

    /// Last epoch power cron tick has been processed.
    pub last_processed_cron_epoch: ChainEpoch,

    /// Claimed power for each miner.
    pub claims: Cid, // Map, HAMT[address]Claim

    pub proof_validation_batch: Option<Cid>,
}

impl State {
    pub fn new(empty_map_cid: Cid, empty_mmap_cid: Cid) -> State {
        State {
            cron_event_queue: empty_mmap_cid,
            claims: empty_map_cid,
            last_processed_cron_epoch: EPOCH_UNDEFINED,
            this_epoch_qa_power_smoothed: FilterEstimate {
                position: INITIAL_QA_POWER_ESTIMATE_POSITION.clone(),
                velocity: INITIAL_QA_POWER_ESTIMATE_VELOCITY.clone(),
            },
            ..Default::default()
        }
    }

    /// Checks power actor state for if miner meets minimum consensus power.
    pub fn miner_nominal_power_meets_consensus_minimum<BS: BlockStore>(
        &self,
        s: &BS,
        miner: &Address,
    ) -> Result<bool, Box<dyn StdError>> {
        let claims = make_map_with_root(&self.claims, s)?;

        let claim =
            get_claim(&claims, miner)?.ok_or_else(|| format!("no claim for actor: {}", miner))?;

        let miner_nominal_power = &claim.quality_adj_power;

        if miner_nominal_power >= &CONSENSUS_MINER_MIN_POWER {
            // If miner is larger than min power requirement, valid
            Ok(true)
        } else if self.miner_above_min_power_count >= CONSENSUS_MINER_MIN_MINERS {
            // if min consensus miners requirement met, return false
            Ok(false)
        } else {
            // if fewer miners than consensus minimum, return true if non-zero power
            Ok(miner_nominal_power.sign() == Sign::Plus)
        }
    }

    pub(super) fn add_to_claim<BS: BlockStore>(
        &mut self,
        claims: &mut Map<BS, Claim>,
        miner: &Address,
        power: &StoragePower,
        qa_power: &StoragePower,
    ) -> Result<(), Box<dyn StdError>> {
        let old_claim = get_claim(claims, miner)?
            .ok_or_else(|| actor_error!(ErrNotFound; "no claim for actor {}", miner))?;

        self.total_qa_bytes_committed += qa_power;
        self.total_bytes_committed += power;

        let new_claim = Claim {
            raw_byte_power: old_claim.raw_byte_power.clone() + power,
            quality_adj_power: old_claim.quality_adj_power.clone() + qa_power,
        };

        let min_power_ref: &StoragePower = &*CONSENSUS_MINER_MIN_POWER;
        let prev_below: bool = &old_claim.quality_adj_power < min_power_ref;
        let still_below: bool = &new_claim.quality_adj_power < min_power_ref;

        if prev_below && !still_below {
            // Just passed min miner size
            self.miner_above_min_power_count += 1;
            self.total_quality_adj_power += &new_claim.quality_adj_power;
            self.total_raw_byte_power += &new_claim.raw_byte_power;
        } else if !prev_below && still_below {
            // just went below min miner size
            self.miner_above_min_power_count -= 1;
            self.total_quality_adj_power = self
                .total_quality_adj_power
                .checked_sub(&old_claim.quality_adj_power)
                .expect("Negative nominal power");
            self.total_raw_byte_power = self
                .total_raw_byte_power
                .checked_sub(&old_claim.raw_byte_power)
                .expect("Negative raw byte power");
        } else if !prev_below && !still_below {
            // Was above the threshold, still above
            self.total_quality_adj_power += qa_power;
            self.total_raw_byte_power += power;
        }

        assert_ne!(
            new_claim.raw_byte_power.sign(),
            Sign::Minus,
            "negative claimed raw byte power: {}",
            new_claim.raw_byte_power
        );
        assert_ne!(
            new_claim.quality_adj_power.sign(),
            Sign::Minus,
            "negative claimed quality adjusted power: {}",
            new_claim.quality_adj_power
        );
        assert!(
            self.miner_above_min_power_count >= 0,
            "negative number of miners larger than min: {}",
            self.miner_above_min_power_count
        );

        Ok(set_claim(claims, miner, new_claim)?)
    }

    pub(super) fn add_pledge_total(&mut self, amount: TokenAmount) {
        self.total_pledge_collateral += amount;
        assert_ne!(self.total_pledge_collateral.sign(), Sign::Minus);
    }

    pub(super) fn append_cron_event<BS: BlockStore>(
        &mut self,
        events: &mut Multimap<BS>,
        epoch: ChainEpoch,
        event: CronEvent,
    ) -> Result<(), Box<dyn StdError>> {
        if epoch < self.first_cron_epoch {
            self.first_cron_epoch = epoch;
        }

        events.add(epoch_key(epoch), event).map_err(|e| {
            e.downcast_wrap(format!("failed to store cron event at epoch {}", epoch))
        })?;
        Ok(())
    }

    pub(super) fn current_total_power(&self) -> (StoragePower, StoragePower) {
        if self.miner_above_min_power_count < CONSENSUS_MINER_MIN_MINERS {
            (
                self.total_bytes_committed.clone(),
                self.total_qa_bytes_committed.clone(),
            )
        } else {
            (
                self.total_raw_byte_power.clone(),
                self.total_quality_adj_power.clone(),
            )
        }
    }

    pub(super) fn update_smoothed_estimate(&mut self, delta: ChainEpoch) {
        let filter_qa_power = AlphaBetaFilter::load(
            &self.this_epoch_qa_power_smoothed,
            &*DEFAULT_ALPHA,
            &*DEFAULT_BETA,
        );
        self.this_epoch_qa_power_smoothed =
            filter_qa_power.next_estimate(&self.this_epoch_quality_adj_power, delta);
    }
}

pub(super) fn load_cron_events<BS: BlockStore>(
    mmap: &Multimap<BS>,
    epoch: ChainEpoch,
) -> Result<Vec<CronEvent>, Box<dyn StdError>> {
    let mut events = Vec::new();

    mmap.for_each(&epoch_key(epoch), |_, v: &CronEvent| {
        events.push(v.clone());
        Ok(())
    })?;

    Ok(events)
}

/// Gets claim from claims map by address
pub fn get_claim<BS: BlockStore>(
    claims: &Map<BS, Claim>,
    a: &Address,
) -> Result<Option<Claim>, Box<dyn StdError>> {
    Ok(claims
        .get(&a.to_bytes())
        .map_err(|e| e.downcast_wrap(format!("failed to get claim for address {}", a)))?)
}

pub fn set_claim<BS: BlockStore>(
    claims: &mut Map<BS, Claim>,
    a: &Address,
    claim: Claim,
) -> Result<(), Box<dyn StdError>> {
    assert_ne!(claim.raw_byte_power.sign(), Sign::Minus);
    assert_ne!(claim.quality_adj_power.sign(), Sign::Minus);

    Ok(claims
        .set(a.to_bytes().into(), claim)
        .map_err(|e| e.downcast_wrap(format!("failed to set claim for address {}", a)))?)
}

pub(super) fn epoch_key(e: ChainEpoch) -> BytesKey {
    let bz = e.encode_var_vec();
    bz.into()
}

impl Cbor for State {}

#[derive(Default, Debug, Serialize_tuple, Deserialize_tuple, Clone)]
pub struct Claim {
    /// Sum of raw byte power for a miner's sectors.
    #[serde(with = "bigint_ser")]
    pub raw_byte_power: StoragePower,
    /// Sum of quality adjusted power for a miner's sectors.
    #[serde(with = "bigint_ser")]
    pub quality_adj_power: StoragePower,
}

#[derive(Clone, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct CronEvent {
    pub miner_addr: Address,
    pub callback_payload: Serialized,
}

impl Cbor for CronEvent {}

#[cfg(test)]
mod test {
    use super::*;
    use clock::ChainEpoch;

    #[test]
    fn epoch_key_test() {
        let e1: ChainEpoch = 101;
        let e2: ChainEpoch = 102;
        let e3: ChainEpoch = 103;
        let e4: ChainEpoch = -1;

        let b1: BytesKey = [0xca, 0x1].to_vec().into();
        let b2: BytesKey = [0xcc, 0x1].to_vec().into();
        let b3: BytesKey = [0xce, 0x1].to_vec().into();
        let b4: BytesKey = [0x1].to_vec().into();

        assert_eq!(b1, epoch_key(e1));
        assert_eq!(b2, epoch_key(e2));
        assert_eq!(b3, epoch_key(e3));
        assert_eq!(b4, epoch_key(e4));
    }
}
