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

#[derive(Default, Serialize, Deserialize, Debug, PartialEq)]
pub struct SectorBuilderState {
    pub staged: StagedState,
    pub sealed: SealedState,
}
