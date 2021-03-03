// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{types::SectorOnChainInfo, PowerPair, BASE_REWARD_FOR_DISPUTED_WINDOW_POST};
use crate::{network::*, DealWeight};
use clock::ChainEpoch;
use fil_types::{
    NetworkVersion, RegisteredPoStProof, RegisteredSealProof, SectorQuality, SectorSize,
    StoragePower,
};
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
// WPoStDisputeWindow is the period after a challenge window ends during which
// PoSts submitted during that period may be disputed.
pub const WPOST_DISPUTE_WINDOW: ChainEpoch = 2 * CHAIN_FINALITY;

/// The maximum number of sectors that a miner can have simultaneously active.
/// This also bounds the number of faults that can be declared, etc.
pub const SECTORS_MAX: usize = 32 << 20;

/// Maximum number of partitions that will be assigned to a deadline.
/// For a minimum storage of upto 1Eib, we need 300 partitions per deadline.
/// 48 * 32GiB * 2349 * 300 = 1.00808144 EiB
/// So, to support upto 10Eib storage, we set this to 3000.
pub const MAX_PARTITIONS_PER_DEADLINE: u64 = 3000;

/// Maximum number of control addresses a miner may register.
pub const MAX_CONTROL_ADDRESSES: usize = 10;

/// MaxPeerIDLength is the maximum length allowed for any on-chain peer ID.
/// Most Peer IDs are expected to be less than 50 bytes.
pub const MAX_PEER_ID_LENGTH: usize = 128;

/// MaxMultiaddrData is the maximum amount of data that can be stored in multiaddrs.
pub const MAX_MULTIADDR_DATA: usize = 1024;

pub const MAX_PROVE_COMMIT_SIZE_V4: usize = 1024;
pub const MAX_PROVE_COMMIT_SIZE_V5: usize = 10240;

/// The maximum number of partitions that may be required to be loaded in a single invocation.
/// This limits the number of simultaneous fault, recovery, or sector-extension declarations.
/// With 48 deadlines (half-hour), 200 partitions per declaration permits loading a full EiB of 32GiB
/// sectors with 1 message per epoch within a single half-hour deadline. A miner can of course submit more messages.
pub const ADDRESSED_PARTITIONS_MAX: u64 = MAX_PARTITIONS_PER_DEADLINE;

/// Maximum number of unique "declarations" in batch operations.
pub const DELCARATIONS_MAX: u64 = ADDRESSED_PARTITIONS_MAX;

/// The maximum number of sector infos that may be required to be loaded in a single invocation.
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

/// Epochs after which chain state is final with overwhelming probability (hence the likelihood of two fork of this size is negligible)
/// This is a conservative value that is chosen via simulations of all known attacks.
pub const CHAIN_FINALITY: ChainEpoch = 900;

/// Prefix for sealed sector CIDs (CommR).
pub const SEALED_CID_PREFIX: cid::Prefix = cid::Prefix {
    version: cid::Version::V1,
    codec: cid::FIL_COMMITMENT_SEALED,
    mh_type: cid::POSEIDON_BLS12_381_A1_FC1,
    mh_len: 32,
};

/// List of proof types which can be used when creating new miner actors
pub fn can_pre_commit_seal_proof(proof: RegisteredSealProof, nv: NetworkVersion) -> bool {
    use RegisteredSealProof::*;

    #[cfg(feature = "devnet")]
    {
        if matches!(proof, StackedDRG2KiBV1 | StackedDRG2KiBV1P1) {
            return true;
        }
    }

    if nv >= NetworkVersion::V8 {
        matches!(proof, StackedDRG32GiBV1P1 | StackedDRG64GiBV1P1)
    } else if nv >= NetworkVersion::V7 {
        matches!(
            proof,
            StackedDRG32GiBV1 | StackedDRG64GiBV1 | StackedDRG32GiBV1P1 | StackedDRG64GiBV1P1
        )
    } else {
        matches!(proof, StackedDRG32GiBV1 | StackedDRG64GiBV1)
    }
}

/// Checks whether a seal proof type is supported for new miners and sectors.
pub fn can_extend_seal_proof_type(proof: RegisteredSealProof) -> bool {
    use RegisteredSealProof::*;

    matches!(proof, StackedDRG32GiBV1P1 | StackedDRG64GiBV1P1)
}

/// Maximum duration to allow for the sealing process for seal algorithms.
/// Dependent on algorithm and sector size
pub fn max_prove_commit_duration(proof: RegisteredSealProof) -> Option<ChainEpoch> {
    use RegisteredSealProof::*;
    match proof {
        StackedDRG32GiBV1 | StackedDRG2KiBV1 | StackedDRG8MiBV1 | StackedDRG512MiBV1
        | StackedDRG64GiBV1 | StackedDRG32GiBV1P1 | StackedDRG2KiBV1P1 | StackedDRG8MiBV1P1
        | StackedDRG512MiBV1P1 | StackedDRG64GiBV1P1 => {
            Some(EPOCHS_IN_DAY + PRE_COMMIT_CHALLENGE_DELAY)
        }
        _ => None,
    }
}

/// Maximum duration to allow for the sealing process for seal algorithms.
/// Dependent on algorithm and sector size
pub fn seal_proof_sector_maximum_lifetime(proof: RegisteredSealProof) -> Option<ChainEpoch> {
    use RegisteredSealProof::*;
    match proof {
        StackedDRG32GiBV1 | StackedDRG2KiBV1 | StackedDRG8MiBV1 | StackedDRG512MiBV1
        | StackedDRG64GiBV1 | StackedDRG32GiBV1P1 | StackedDRG2KiBV1P1 | StackedDRG8MiBV1P1
        | StackedDRG512MiBV1P1 | StackedDRG64GiBV1P1 => Some(EPOCHS_IN_YEAR * 5),
        _ => None,
    }
}

pub const MAX_PRE_COMMIT_RANDOMNESS_LOOKBACK: ChainEpoch = EPOCHS_IN_DAY + CHAIN_FINALITY;

/// Number of epochs between publishing the precommit and when the challenge for interactive PoRep is drawn
/// used to ensure it is not predictable by miner.
pub const PRE_COMMIT_CHALLENGE_DELAY: ChainEpoch = 150;

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

/// Number of epochs after a consensus fault for which a miner is ineligible
/// for permissioned actor methods and winning block elections.
pub const CONSENSUS_FAULT_INELIGIBILITY_DURATION: ChainEpoch = CHAIN_FINALITY;

/// DealWeight and VerifiedDealWeight are spacetime occupied by regular deals and verified deals in a sector.
/// Sum of DealWeight and VerifiedDealWeight should be less than or equal to total SpaceTime of a sector.
/// Sectors full of VerifiedDeals will have a SectorQuality of VerifiedDealWeightMultiplier/QualityBaseMultiplier.
/// Sectors full of Deals will have a SectorQuality of DealWeightMultiplier/QualityBaseMultiplier.
/// Sectors with neither will have a SectorQuality of QualityBaseMultiplier/QualityBaseMultiplier.
/// SectorQuality of a sector is a weighted average of multipliers based on their proportions.
fn quality_for_weight(
    size: SectorSize,
    duration: ChainEpoch,
    deal_weight: &DealWeight,
    verified_weight: &DealWeight,
) -> SectorQuality {
    let sector_space_time = BigInt::from(size as u64) * BigInt::from(duration);
    let total_deal_space_time = deal_weight + verified_weight;

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
pub fn sector_deals_max(size: SectorSize) -> u64 {
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

pub const REWARD_VESTING_SPEC: VestSpec = VestSpec {
    initial_delay: 0,                  // PARAM_FINISH
    vest_period: 180 * EPOCHS_IN_DAY,  // PARAM_FINISH
    step_duration: EPOCHS_IN_DAY,      // PARAM_FINISH
    quantization: 12 * EPOCHS_IN_HOUR, // PARAM_FINISH
};

pub fn reward_for_consensus_slash_report(
    elapsed_epoch: ChainEpoch,
    collateral: &TokenAmount,
) -> TokenAmount {
    // var growthRate = SLASHER_SHARE_GROWTH_RATE_NUM / SLASHER_SHARE_GROWTH_RATE_DENOM
    // var multiplier = growthRate^elapsedEpoch
    // var slasherProportion = min(INITIAL_SLASHER_SHARE * multiplier, 1.0)
    // return collateral * slasherProportion
    // BigInt Operation
    // NUM = SLASHER_SHARE_GROWTH_RATE_NUM^elapsedEpoch * INITIAL_SLASHER_SHARE_NUM * collateral
    // DENOM = SLASHER_SHARE_GROWTH_RATE_DENOM^elapsedEpoch * INITIAL_SLASHER_SHARE_DENOM
    // slasher_amount = min(NUM/DENOM, collateral)
    let consensus_fault_reporter_share_growth_rate = BigFrac {
        numerator: BigInt::from(100_785_473_384u64),
        denominator: BigInt::from(100_000_000_000u64),
    };
    let consensus_fault_reporter_initial_share = BigFrac {
        numerator: BigInt::from(1),
        denominator: BigInt::from(1000),
    };
    let max_reporter_share = BigFrac {
        numerator: BigInt::from(1),
        denominator: BigInt::from(20),
    };
    let elapsed = BigUint::from(elapsed_epoch as u64);
    let slasher_share_numerator = consensus_fault_reporter_share_growth_rate
        .numerator
        .pow(&elapsed);
    let slasher_share_denominator = consensus_fault_reporter_share_growth_rate
        .denominator
        .pow(&elapsed);
    let num: BigInt =
        (slasher_share_numerator * consensus_fault_reporter_initial_share.numerator) * collateral;
    let denom = slasher_share_denominator * consensus_fault_reporter_initial_share.denominator;

    cmp::min(
        num.div_floor(&denom),
        (collateral * max_reporter_share.numerator).div_floor(&max_reporter_share.denominator),
    )
}

// The reward given for successfully disputing a window post.
pub fn reward_for_disputed_window_post(
    _proof_type: RegisteredPoStProof,
    _disputed_power: PowerPair,
) -> TokenAmount {
    // This is currently just the base. In the future, the fee may scale based on the disputed power.
    BASE_REWARD_FOR_DISPUTED_WINDOW_POST.clone()
}
