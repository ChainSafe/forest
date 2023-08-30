// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use std::{fmt::Debug, sync::Arc};

use crate::beacon::BeaconSchedule;
use crate::blocks::{Block, Tipset};
use crate::chain::{Error as ChainStoreError, Weight};
use crate::state_manager::{Error as StateManagerError, StateManager};
use anyhow::anyhow;
use fvm_ipld_blockstore::Blockstore;
use fvm_ipld_encoding::Error as ForestEncodingError;
use nonempty::NonEmpty;
use thiserror::Error;

mod metrics;
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
    #[error("Chain store error: {0}")]
    ChainStore(#[from] ChainStoreError),
    #[error("StateManager error: {0}")]
    StateManager(#[from] StateManagerError),
    #[error("Encoding error: {0}")]
    ForestEncoding(#[from] ForestEncodingError),
}

pub struct FilecoinConsensus {
    /// `Drand` randomness beacon
    ///
    /// NOTE: The `StateManager` makes available a beacon as well,
    /// but it potentially has a different type.
    /// Not sure where this is utilized.
    beacon: Arc<BeaconSchedule>,
}

impl FilecoinConsensus {
    pub fn new(beacon: Arc<BeaconSchedule>) -> Self {
        Self { beacon }
    }

    pub async fn validate_block<DB: Blockstore + Sync + Send + 'static>(
        &self,
        state_manager: Arc<StateManager<DB>>,
        block: Arc<Block>,
    ) -> Result<(), NonEmpty<FilecoinConsensusError>> {
        validation::validate_block::<_>(state_manager, self.beacon.clone(), block).await
    }
}

impl Debug for FilecoinConsensus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilecoinConsensus")
            .field("beacon", &self.beacon.0.len())
            .finish()
    }
}

pub fn weight<DB>(db: &DB, ts: &Tipset) -> Result<Weight, anyhow::Error>
where
    DB: Blockstore,
{
    weight::weight(&Arc::new(db), ts).map_err(|s| anyhow!(s))
}
