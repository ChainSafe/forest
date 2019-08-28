use std::fmt;

use filecoin_proofs::types::UnpaddedBytesAmount;
use serde::{Deserialize, Serialize};
use storage_proofs::sector::SectorId;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct StagedSectorMetadata {
    pub sector_id: SectorId,
    pub sector_access: String,
    pub pieces: Vec<PieceMetadata>,
    pub seal_status: SealStatus,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SealedSectorMetadata {
    pub sector_id: SectorId,
    pub sector_access: String,
    pub pieces: Vec<PieceMetadata>,
    pub comm_r_star: [u8; 32],
    pub comm_r: [u8; 32],
    pub comm_d: [u8; 32],
    pub proof: Vec<u8>,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct PieceMetadata {
    pub piece_key: String,
    pub num_bytes: UnpaddedBytesAmount,
    pub comm_p: Option<[u8; 32]>,
    pub piece_inclusion_proof: Option<Vec<u8>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum SealStatus {
    Failed(String),
    Pending,
    Sealed(Box<SealedSectorMetadata>),
    Sealing,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct SecondsSinceEpoch(pub u64);

impl PartialEq for SealedSectorMetadata {
    fn eq(&self, other: &SealedSectorMetadata) -> bool {
        self.sector_id == other.sector_id
            && self.sector_access == other.sector_access
            && self.pieces == other.pieces
            && self.comm_r_star == other.comm_r_star
            && self.comm_r == other.comm_r
            && self.comm_d == other.comm_d
            && self.proof.iter().eq(other.proof.iter())
    }
}

impl Default for StagedSectorMetadata {
    fn default() -> StagedSectorMetadata {
        StagedSectorMetadata {
            sector_id: Default::default(),
            sector_access: Default::default(),
            pieces: Default::default(),
            seal_status: SealStatus::Pending,
        }
    }
}

impl fmt::Debug for SealedSectorMetadata {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "SealedSectorMetadata {{ sector_id: {}, sector_access: {}, pieces: {:?}, comm_r_star: {:?}, comm_r: {:?}, comm_d: {:?} }}", self.sector_id, self.sector_access, self.pieces, self.comm_r_star, self.comm_r, self.comm_d)
    }
}

impl Default for SealedSectorMetadata {
    fn default() -> SealedSectorMetadata {
        SealedSectorMetadata {
            sector_id: Default::default(),
            sector_access: Default::default(),
            pieces: Default::default(),
            comm_r_star: Default::default(),
            comm_r: Default::default(),
            comm_d: Default::default(),
            proof: Default::default(),
        }
    }
}
