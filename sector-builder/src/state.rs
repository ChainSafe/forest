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
