// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
#![allow(unused_variables)]
#![allow(dead_code)]
use super::types::SectorOnChainInfo;
use clock::ChainEpoch;
use fil_types::{
    RegisteredProof, RegisteredProof::StackedDRG32GiBSeal, RegisteredProof::StackedDRG64GiBSeal,
    SectorSize,
};
use num_bigint::BigUint;
use num_traits::Zero;
use vm::TokenAmount;
/// The duration of a chain epoch.
/// This is used for deriving epoch-denominated periods that are more naturally expressed in clock time.
pub const EPOCH_DURATION_SECONDS: u64 = 25;
pub const SECONDS_IN_YEAR: u64 = 31_556_925;
pub const SECONDS_IN_DAY: u64 = 86400;
/// The period over which all a miner's active sectors will be challenged.
pub const WPOST_PROVING_PERIOD: ChainEpoch = SECONDS_IN_DAY / EPOCH_DURATION_SECONDS;
/// The duration of a deadline's challenge window, the period before a deadline when the challenge is available.
pub const WPOST_CHALLENGE_WINDOW: ChainEpoch = 1800 / EPOCH_DURATION_SECONDS; // Half an hour (=48 per day)
/// The number of non-overlapping PoSt deadlines in each proving period.
pub const WPOST_PERIOD_DEADLINES: u64 = WPOST_PROVING_PERIOD / WPOST_CHALLENGE_WINDOW;
/// The maximum number of sectors in a single window PoSt proof.
pub const WPOST_PARTITION_SECTORS: u64 = 2350;
/// The maximum number of partitions that may be submitted in a single message.
/// This bounds the size of a list/set of sector numbers that might be instantiated to process a submission.
pub const WPOST_MESSAGE_PARTITIONS_MAX: u64 = 100_000 / WPOST_PARTITION_SECTORS;
/// The maximum number of sectors that a miner can have simultaneously active.
/// This also bounds the number of faults that can be declared, etc.
/// TODO raise this number, carefully
const SECTORS_MAX: u64 = 32 << 20; // PARAM_FINISH
/// The maximum number of proving partitions a miner can have simultaneously active.
pub fn active_partitions_max(partition_sector_count: u64) -> u64 {
    (SECTORS_MAX / partition_sector_count) + WPOST_PERIOD_DEADLINES
}
/// The maximum number of partitions that may be submitted in a single message.
/// This bounds the size of a list/set of sector numbers that might be instantiated to process a submission.
pub fn window_post_message_partitions_max(partition_sector_count: u64) -> u64 {
    100_000 / partition_sector_count
}
/// The maximum number of proving partitions a miner can have simultaneously active.
pub const PARTITIONS_MAX: u64 = (SECTORS_MAX / WPOST_PARTITION_SECTORS) + WPOST_PERIOD_DEADLINES;
/// The maximum number of new sectors that may be staged by a miner during a single proving period.
pub const NEW_SECTORS_PER_PERIOD_MAX: u64 = 128 << 10;
/// An approximation to chain state finality (should include message propagation time as well).
pub const CHAIN_FINALITYISH: ChainEpoch = 500; // PARAM_FINISH
/// List of proof types which can be used when creating new miner actors
pub enum SupportedProofTypes {
    StackedDRG32GiBSeal,
    StackedDRG64GiBSeal,
}
/// List of proof types which can be used when creating new miner actors
pub fn check_supported_proof_types(proof: RegisteredProof) -> bool {
    match proof {
        StackedDRG32GiBSeal => true,
        StackedDRG64GiBSeal => true,
        _ => false,
    }
}
/// Maximum duration to allow for the sealing process for seal algorithms.
/// Dependent on algorithm and sector size
pub fn max_seal_duration(proof: &RegisteredProof) -> Option<ChainEpoch> {
    match proof {
        RegisteredProof::StackedDRG32GiBSeal => Some(10000),
        RegisteredProof::StackedDRG2KiBSeal => Some(10000),
        RegisteredProof::StackedDRG8MiBSeal => Some(10000),
        RegisteredProof::StackedDRG512MiBSeal => Some(10000),
        RegisteredProof::StackedDRG64GiBSeal => Some(10000),
        _ => None,
    }
}
/// Number of epochs between publishing the precommit and when the challenge for interactive PoRep is drawn
/// used to ensure it is not predictable by miner.
pub const PRE_COMMIT_CHALLENGE_DELAY: ChainEpoch = 10;
/// Lookback from the current epoch for state view for leader elections.
pub const ELECTION_LOOKBACK: ChainEpoch = 1; // PARAM_FINISH
/// Lookback from the deadline's challenge window opening from which to sample chain randomness for the challenge seed.
pub const WPOST_CHALLENGE_LOOKBACK: ChainEpoch = 20; // PARAM_FINISH
/// Minimum period before a deadline's challenge window opens that a fault must be declared for that deadline.
/// A fault declaration may appear in the challenge epoch, since it must have been posted before the
/// epoch completed, and hence before the challenge was knowable.
pub const FAULT_DECLARATION_CUTOFF: ChainEpoch = WPOST_CHALLENGE_LOOKBACK; // PARAM_FINISH
/// The maximum age of a fault before the sector is terminated.
pub const FAULT_MAX_AGE: ChainEpoch = WPOST_PROVING_PERIOD * 14 - 1;
/// Staging period for a miner worker key change.
pub const WORKER_KEY_CHANGE_DELAY: ChainEpoch = 2 * ELECTION_LOOKBACK; // PARAM_FINISH
/// Deposit per sector required at pre-commitment, refunded after the commitment is proven (else burned).
pub fn precommit_deposit(sector_size: SectorSize, _duration: &ChainEpoch) -> TokenAmount {
    let deposit_per_byte = BigUint::zero(); // PARAM_FINISH
    deposit_per_byte * BigUint::from(sector_size as u64)
}
struct BigFrac {
    numerator: BigUint,
    denominator: BigUint,
}
fn pledge_penalty_for_sector_termination(_sector: SectorOnChainInfo) -> TokenAmount {
    BigUint::zero() // PARAM_FINISH
}
/// Penalty to locked pledge collateral for a "skipped" sector or missing PoSt fault.
pub fn pledge_penalty_for_sector_undeclared_fault() -> TokenAmount {
    BigUint::zero() // PARAM_FINISH
}
/// Penalty to locked pledge collateral for a declared or on-going sector fault.
pub fn pledge_penalty_for_sector_declared_fault(_sector: SectorOnChainInfo) -> TokenAmount {
    BigUint::zero() // PARAM_FINISH
}
/// Specification for a linear vesting schedule.
pub struct VestSpec {
    pub initial_delay: ChainEpoch, // Delay before any amount starts vesting.
    pub vest_period: ChainEpoch, // Period over which the total should vest, after the initial delay.
    pub step_duration: ChainEpoch, // Duration between successive incremental vests (independent of vesting period).
    pub quantization: ChainEpoch, // Maximum precision of vesting table (limits cardinality of table).
}
pub const PLEDGE_VESTING_SPEC: VestSpec = VestSpec {
    initial_delay: SECONDS_IN_YEAR / EPOCH_DURATION_SECONDS, // 1 year, PARAM_FINISH
    vest_period: SECONDS_IN_YEAR / EPOCH_DURATION_SECONDS,   // 1 year, PARAM_FINISH
    step_duration: 7 * SECONDS_IN_YEAR / EPOCH_DURATION_SECONDS, // 1 week, PARAM_FINISH
    quantization: SECONDS_IN_DAY / EPOCH_DURATION_SECONDS,   // 1 day, PARAM_FINISH
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
        numerator: BigUint::from(101_251 as u64),
        denominator: BigUint::from(100_000 as u64),
    };
    let consensus_fault_reporter_initial_share = BigFrac {
        // PARAM_FINISH
        numerator: BigUint::from(1 as u64),
        denominator: BigUint::from(1000 as u64),
    };
    let max_reporter_share_num = BigUint::from(1 as u64);
    let max_reporter_share_den = BigUint::from(2 as u64);
    let elapsed = BigUint::from(elapsed_epoch);
    let slasher_share_numerator = consensus_fault_reporter_share_growth_rate.numerator ^ &elapsed;
    let slasher_share_denominator =
        consensus_fault_reporter_share_growth_rate.denominator ^ &elapsed;
    let num =
        (slasher_share_numerator * consensus_fault_reporter_initial_share.numerator) * &collateral;
    let denom = slasher_share_denominator * consensus_fault_reporter_initial_share.denominator;
    std::cmp::min(
        num / denom,
        (collateral * max_reporter_share_num) / max_reporter_share_den,
    )
}