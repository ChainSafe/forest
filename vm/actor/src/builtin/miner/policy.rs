// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::types::SectorOnChainInfo;
use crate::{network::*, DealWeight};
use clock::ChainEpoch;
use fil_types::{RegisteredSealProof, SectorQuality, SectorSize, StoragePower};
use num_bigint::BigUint;
use num_bigint::{BigInt, Integer};
use num_traits::Pow;
use std::cmp;
use vm::TokenAmount;

/// The period over which all a miner's active sectors will be challenged.
pub const WPOST_PROVING_PERIOD: ChainEpoch = EPOCHS_IN_DAY;
/// The duration of a deadline's challenge window, the period before a deadline when the challenge is available.
pub const WPOST_CHALLENGE_WINDOW: ChainEpoch = 30 * 60 / EPOCH_DURATION_SECONDS; // Half an hour (=48 per day)
/// The number of non-overlapping PoSt deadlines in each proving period.
pub const WPOST_PERIOD_DEADLINES: u64 = 48;
/// The maximum distance back that a valid Window PoSt must commit to the current chain.
pub const WPOST_MAX_CHAIN_COMMIT_AGE: ChainEpoch = WPOST_CHALLENGE_WINDOW;

// The maximum number of sectors that a miner can have simultaneously active.
// This also bounds the number of faults that can be declared, etc.
pub const SECTORS_MAX: usize = 32 << 20; // PARAM_FINISH

// The maximum number of partitions that may be required to be loaded in a single invocation.
// This limits the number of simultaneous fault, recovery, or sector-extension declarations.
// With 48 deadlines (half-hour), 200 partitions per declaration permits loading a full EiB of 32GiB
// sectors with 1 message per epoch within a single half-hour deadline. A miner can of course submit more messages.
pub const ADDRESSED_PARTITIONS_MAX: u64 = 200;

// The maximum number of sector infos that may be required to be loaded in a single invocation.
pub const ADDRESSED_SECTORS_MAX: u64 = 10_000;

/// The maximum number of partitions that may be required to be loaded in a single invocation,
/// when all the sector infos for the partitions will be loaded.
pub fn load_partitions_sectors_max(partition_sector_count: u64) -> u64 {
    cmp::min(
        ADDRESSED_SECTORS_MAX / partition_sector_count,
        ADDRESSED_PARTITIONS_MAX,
    )
}

/// The maximum number of new sectors that may be staged by a miner during a single proving period.
pub const NEW_SECTORS_PER_PERIOD_MAX: usize = 128 << 10;

/// Epochs after which chain state is final.
pub const CHAIN_FINALITY: ChainEpoch = 900;

pub const SEALED_CID_PREFIX: cid::Prefix = cid::Prefix {
    version: cid::Version::V1,
    codec: cid::Codec::FilCommitmentSealed,
    mh_type: cid::POSEIDON_BLS12_381_A1_FC1,
    mh_len: 32,
};

/// List of proof types which can be used when creating new miner actors
pub fn check_supported_proof_types(proof: RegisteredSealProof) -> bool {
    use RegisteredSealProof::*;
    matches!(proof, StackedDRG32GiBV1 | StackedDRG64GiBV1)
}
/// Maximum duration to allow for the sealing process for seal algorithms.
/// Dependent on algorithm and sector size
pub fn max_seal_duration(proof: RegisteredSealProof) -> Option<ChainEpoch> {
    use RegisteredSealProof::*;
    match proof {
        StackedDRG32GiBV1 | StackedDRG2KiBV1 | StackedDRG8MiBV1 | StackedDRG512MiBV1
        | StackedDRG64GiBV1 => Some(10000),
        _ => None,
    }
}
/// Number of epochs between publishing the precommit and when the challenge for interactive PoRep is drawn
/// used to ensure it is not predictable by miner.
pub const PRE_COMMIT_CHALLENGE_DELAY: ChainEpoch = 150;

/// Lookback from the current epoch for state view for leader elections.
pub const ELECTION_LOOKBACK: ChainEpoch = 1; // PARAM_FINISH

/// Lookback from the deadline's challenge window opening from which to sample chain randomness for the challenge seed.

/// This lookback exists so that deadline windows can be non-overlapping (which make the programming simpler)
/// but without making the miner wait for chain stability before being able to start on PoSt computation.
/// The challenge is available this many epochs before the window is actually open to receiving a PoSt.
pub const WPOST_CHALLENGE_LOOKBACK: ChainEpoch = 20;

/// Minimum period before a deadline's challenge window opens that a fault must be declared for that deadline.
/// This lookback must not be less than WPoStChallengeLookback lest a malicious miner be able to selectively declare
/// faults after learning the challenge value.
pub const FAULT_DECLARATION_CUTOFF: ChainEpoch = WPOST_CHALLENGE_LOOKBACK + 50;

/// The maximum age of a fault before the sector is terminated.
pub const FAULT_MAX_AGE: ChainEpoch = WPOST_PROVING_PERIOD * 14;

/// Staging period for a miner worker key change.
/// Finality is a harsh delay for a miner who has lost their worker key, as the miner will miss Window PoSts until
/// it can be changed. It's the only safe value, though. We may implement a mitigation mechanism such as a second
/// key or allowing the owner account to submit PoSts while a key change is pending.
pub const WORKER_KEY_CHANGE_DELAY: ChainEpoch = CHAIN_FINALITY;

/// Minimum number of epochs past the current epoch a sector may be set to expire.
pub const MIN_SECTOR_EXPIRATION: i64 = 180 * EPOCHS_IN_DAY;

/// Maximum number of epochs past the current epoch a sector may be set to expire.
/// The actual maximum extension will be the minimum of CurrEpoch + MaximumSectorExpirationExtension
/// and sector.ActivationEpoch+sealProof.SectorMaximumLifetime()
pub const MAX_SECTOR_EXPIRATION_EXTENSION: i64 = 540 * EPOCHS_IN_DAY;

/// Ratio of sector size to maximum deals per sector.
/// The maximum number of deals is the sector size divided by this number (2^27)
/// which limits 32GiB sectors to 256 deals and 64GiB sectors to 512
pub const DEAL_LIMIT_DENOMINATOR: u64 = 134217728;

/// DealWeight and VerifiedDealWeight are spacetime occupied by regular deals and verified deals in a sector.
/// Sum of DealWeight and VerifiedDealWeight should be less than or equal to total SpaceTime of a sector.
/// Sectors full of VerifiedDeals will have a SectorQuality of VerifiedDealWeightMultiplier/QualityBaseMultiplier.
/// Sectors full of Deals will have a SectorQuality of DealWeightMultiplier/QualityBaseMultiplier.
/// Sectors with neither will have a SectorQuality of QualityBaseMultiplier/QualityBaseMultiplier.
/// SectorQuality of a sector is a weighted average of multipliers based on their propotions.
fn quality_for_weight(
    size: SectorSize,
    duration: ChainEpoch,
    deal_weight: &DealWeight,
    verified_weight: &DealWeight,
) -> SectorQuality {
    let sector_space_time = BigInt::from(size as u64) * BigInt::from(duration);
    let total_deal_space_time = deal_weight + verified_weight;
    assert!(sector_space_time >= total_deal_space_time);

    let weighted_base_space_time =
        (&sector_space_time - total_deal_space_time) * &*QUALITY_BASE_MULTIPLIER;
    let weighted_deal_space_time = deal_weight * &*DEAL_WEIGHT_MULTIPLIER;
    let weighted_verified_space_time = verified_weight * &*VERIFIED_DEAL_WEIGHT_MULTIPLIER;
    let weighted_sum_space_time =
        weighted_base_space_time + weighted_deal_space_time + weighted_verified_space_time;
    let scaled_up_weighted_sum_space_time: SectorQuality =
        weighted_sum_space_time << SECTOR_QUALITY_PRECISION;

    scaled_up_weighted_sum_space_time
        .div_floor(&sector_space_time)
        .div_floor(&QUALITY_BASE_MULTIPLIER)
}

/// Returns the power for a sector size and weight.
pub fn qa_power_for_weight(
    size: SectorSize,
    duration: ChainEpoch,
    deal_weight: &DealWeight,
    verified_weight: &DealWeight,
) -> StoragePower {
    let quality = quality_for_weight(size, duration, deal_weight, verified_weight);
    (BigInt::from(size as u64) * quality) >> SECTOR_QUALITY_PRECISION
}

/// Returns the quality-adjusted power for a sector.
pub fn qa_power_for_sector(size: SectorSize, sector: &SectorOnChainInfo) -> StoragePower {
    let duration = sector.expiration - sector.activation;
    qa_power_for_weight(
        size,
        duration,
        &sector.deal_weight,
        &sector.verified_deal_weight,
    )
}

/// Determine maximum number of deal miner's sector can hold
pub fn deal_per_sector_limit(size: SectorSize) -> u64 {
    cmp::max(256, size as u64 / DEAL_LIMIT_DENOMINATOR)
}

struct BigFrac {
    numerator: BigInt,
    denominator: BigInt,
}

/// Specification for a linear vesting schedule.
pub struct VestSpec {
    pub initial_delay: ChainEpoch, // Delay before any amount starts vesting.
    pub vest_period: ChainEpoch, // Period over which the total should vest, after the initial delay.
    pub step_duration: ChainEpoch, // Duration between successive incremental vests (independent of vesting period).
    pub quantization: ChainEpoch, // Maximum precision of vesting table (limits cardinality of table).
}

pub const PLEDGE_VESTING_SPEC: VestSpec = VestSpec {
    initial_delay: 180 * EPOCHS_IN_DAY, // PARAM_FINISH
    vest_period: 180 * EPOCHS_IN_DAY,   // PARAM_FINISH
    step_duration: EPOCHS_IN_DAY,       // PARAM_FINISH
    quantization: 12 * EPOCHS_IN_HOUR,  // PARAM_FINISH
};

pub const REWARD_VESTING_SPEC_V0: VestSpec = VestSpec {
    initial_delay: 20 * EPOCHS_IN_DAY, // PARAM_FINISH
    vest_period: 180 * EPOCHS_IN_DAY,  // PARAM_FINISH
    step_duration: EPOCHS_IN_DAY,      // PARAM_FINISH
    quantization: 12 * EPOCHS_IN_HOUR, // PARAM_FINISH
};

pub const REWARD_VESTING_SPEC_V1: VestSpec = VestSpec {
    initial_delay: 0,                  // PARAM_FINISH
    vest_period: 180 * EPOCHS_IN_DAY,  // PARAM_FINISH
    step_duration: EPOCHS_IN_DAY,      // PARAM_FINISH
    quantization: 12 * EPOCHS_IN_HOUR, // PARAM_FINISH
};

pub fn reward_for_consensus_slash_report(
    elapsed_epoch: ChainEpoch,
    collateral: TokenAmount,
) -> TokenAmount {
    // PARAM_FINISH
    // var growthRate = SLASHER_SHARE_GROWTH_RATE_NUM / SLASHER_SHARE_GROWTH_RATE_DENOM
    // var multiplier = growthRate^elapsedEpoch
    // var slasherProportion = min(INITIAL_SLASHER_SHARE * multiplier, 1.0)
    // return collateral * slasherProportion
    // BigInt Operation
    // NUM = SLASHER_SHARE_GROWTH_RATE_NUM^elapsedEpoch * INITIAL_SLASHER_SHARE_NUM * collateral
    // DENOM = SLASHER_SHARE_GROWTH_RATE_DENOM^elapsedEpoch * INITIAL_SLASHER_SHARE_DENOM
    // slasher_amount = min(NUM/DENOM, collateral)
    let consensus_fault_reporter_share_growth_rate = BigFrac {
        // PARAM_FINISH
        numerator: BigInt::from(101_251 as u64),
        denominator: BigInt::from(100_000 as u64),
    };
    let consensus_fault_reporter_initial_share = BigFrac {
        // PARAM_FINISH
        numerator: BigInt::from(1 as u64),
        denominator: BigInt::from(1000 as u64),
    };
    let max_reporter_share_num = BigInt::from(1 as u64);
    let max_reporter_share_den = BigInt::from(2 as u64);
    let elapsed = BigUint::from(elapsed_epoch as u64);
    let slasher_share_numerator = consensus_fault_reporter_share_growth_rate
        .numerator
        .pow(&elapsed);
    let slasher_share_denominator = consensus_fault_reporter_share_growth_rate
        .denominator
        .pow(&elapsed);
    let num: BigInt =
        (slasher_share_numerator * consensus_fault_reporter_initial_share.numerator) * &collateral;
    let denom = slasher_share_denominator * consensus_fault_reporter_initial_share.denominator;

    cmp::min(
        num.div_floor(&denom),
        (collateral * max_reporter_share_num).div_floor(&max_reporter_share_den),
    )
}
