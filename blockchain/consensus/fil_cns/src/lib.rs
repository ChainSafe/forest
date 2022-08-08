// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use anyhow::anyhow;
use async_trait::async_trait;
use std::fmt::Debug;
use std::{marker::PhantomData, sync::Arc};
use thiserror::Error;

use beacon::{Beacon, BeaconSchedule};
use chain::Weight;
use chain::{Error as ChainStoreError, Scale};
use chain_sync::Consensus;
use fil_types::verifier::ProofVerifier;
use forest_blocks::{Block, Tipset};
use fvm_ipld_encoding::Error as ForestEncodingError;
use ipld_blockstore::BlockStore;
use nonempty::NonEmpty;
use state_manager::Error as StateManagerError;
use state_manager::StateManager;

mod validation;
mod weight;

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
    #[error("Block is not claiming to be a winner")]
    NotClaimingWin,
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
    #[error("Failed to validate blocks random beacon values: {0}")]
    BeaconValidation(String),
    #[error("Failed to verify winning PoSt: {0}")]
    WinningPoStValidation(String),
    #[error("[INSECURE-POST-VALIDATION] {0}")]
    InsecurePostValidation(String),
    #[error("Chain store error: {0}")]
    ChainStore(#[from] ChainStoreError),
    #[error("StateManager error: {0}")]
    StateManager(#[from] StateManagerError),
    #[error("Encoding error: {0}")]
    ForestEncoding(#[from] ForestEncodingError),
}

pub struct FilecoinConsensus<B, V> {
    /// `Drand` randomness beacon
    ///
    /// NOTE: The `StateManager` makes available a beacon as well,
    /// but it potentially has a different type.
    /// Not sure where this is utilized.
    beacon: Arc<BeaconSchedule<B>>,
    /// Proof verification implementation.
    verifier: PhantomData<V>,
}

impl<B, V> FilecoinConsensus<B, V> {
    pub fn new(beacon: Arc<BeaconSchedule<B>>) -> Self {
        Self {
            beacon,
            verifier: PhantomData,
        }
    }
}

impl<B, V> Debug for FilecoinConsensus<B, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilecoinConsensus")
            .field("beacon", &self.beacon.0.len())
            .field("verifier", &self.verifier)
            .finish()
    }
}

impl<B, V> Scale for FilecoinConsensus<B, V> {
    fn weight<DB>(db: &DB, ts: &Tipset) -> Result<Weight, anyhow::Error>
    where
        DB: BlockStore,
    {
        weight::weight(db, ts).map_err(|s| anyhow!(s))
    }
}

#[async_trait]
impl<B, V> Consensus for FilecoinConsensus<B, V>
where
    B: Beacon + Unpin + Send + Sync + 'static,
    V: ProofVerifier + Unpin + Send + Sync + 'static,
{
    type Error = FilecoinConsensusError;

    async fn validate_block<DB>(
        &self,
        state_manager: Arc<StateManager<DB>>,
        block: Arc<Block>,
    ) -> Result<(), NonEmpty<Self::Error>>
    where
        DB: BlockStore + Sync + Send + 'static,
    {
        validation::validate_block::<_, _, V>(state_manager, self.beacon.clone(), block).await
    }
}
