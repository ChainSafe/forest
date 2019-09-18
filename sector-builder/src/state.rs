use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use storage_proofs::sector::SectorId;

use crate::metadata::{SealedSectorMetadata, StagedSectorMetadata};

#[derive(Default, Serialize, Deserialize, Debug, PartialEq)]
pub struct StagedState {
    pub sector_id_nonce: u64,
    pub sectors: HashMap<SectorId, StagedSectorMetadata>,
}

#[derive(Default, Serialize, Deserialize, Debug, PartialEq)]
pub struct SealedState {
    pub sectors: HashMap<SectorId, SealedSectorMetadata>,
}

#[derive(Default, Serialize, Deserialize, Debug, PartialEq)]
pub struct SectorBuilderState {
    pub staged: StagedState,
    pub sealed: SealedState,
}

impl SectorBuilderState {
    pub fn new(last_committed_sector_id: SectorId) -> SectorBuilderState {
        SectorBuilderState {
            staged: StagedState {
                sector_id_nonce: u64::from(last_committed_sector_id),
                sectors: Default::default(),
            },
            sealed: Default::default(),
        }
    }
}
