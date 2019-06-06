use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::builder::SectorId;
use crate::metadata::{SealedSectorMetadata, StagedSectorMetadata};

#[derive(Default, Serialize, Deserialize, Debug, PartialEq)]
pub struct StagedState {
    pub sector_id_nonce: SectorId,
    pub sectors: HashMap<SectorId, StagedSectorMetadata>,
}

#[derive(Default, Serialize, Deserialize, Debug, PartialEq)]
pub struct SealedState {
    pub sectors: HashMap<SectorId, SealedSectorMetadata>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SectorBuilderState {
    pub prover_id: [u8; 31],
    pub staged: StagedState,
    pub sealed: SealedState,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct StateSnapshot {
    pub prover_id: [u8; 31],
    pub staged: StagedState,
    pub sealed: SealedState,
}

impl Into<SectorBuilderState> for StateSnapshot {
    fn into(self) -> SectorBuilderState {
        SectorBuilderState {
            prover_id: self.prover_id,
            staged: self.staged,
            sealed: self.sealed,
        }
    }
}
