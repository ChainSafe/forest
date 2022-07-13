use chain_sync::Consensus;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FilecoinConsensusError {
    #[error("Block must have an election proof included in tipset")]
    BlockWithoutElectionProof,
    #[error("Block without ticket")]
    BlockWithoutTicket,
    #[error("Block had the wrong timestamp: {0} != {1}")]
    UnequalBlockTimestamps(u64, u64),
    #[error("Tipset without ticket to verify")]
    TipsetWithoutTicket,
    #[error("Winner election proof verification failed: {0}")]
    WinnerElectionProofVerificationFailed(String),
    #[error("Block miner was slashed or is invalid")]
    InvalidOrSlashedMiner,
    #[error("Miner power not available for miner address")]
    MinerPowerNotAvailable,
    #[error("Miner claimed wrong number of wins: miner = {0}, computed = {1}")]
    MinerWinClaimsIncorrect(i64, i64),
    #[error("Drawing chain randomness failed: {0}")]
    DrawingChainRandomness(String),
    #[error("Miner isn't elligible to mine")]
    MinerNotEligibleToMine,
    #[error("Querying miner power failed: {0}")]
    MinerPowerUnavailable(String),
    #[error("Power actor not found")]
    PowerActorUnavailable,
    #[error("Verifying VRF failed: {0}")]
    VrfValidation(String),
    #[error("[INSECURE-POST-VALIDATION] {0}")]
    InsecurePostValidation(String),
}

#[derive(Debug)]
pub struct FilecoinConsensus;

impl Consensus for FilecoinConsensus {
    type Error = FilecoinConsensusError;
}
