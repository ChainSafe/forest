use crate::TemporaryAuxKey;
use filecoin_proofs::types::UnpaddedBytesAmount;
use filecoin_proofs::{Commitment, PersistentAux, PieceInfo};
use serde::{Deserialize, Serialize};
use storage_proofs::sector::SectorId;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct StagedSectorMetadata {
    pub sector_id: SectorId,
    pub sector_access: String,
    pub pieces: Vec<PieceMetadata>,
    pub seal_status: SealStatus,
}

#[derive(Clone, Serialize, Deserialize, Default, PartialEq, Debug)]
pub struct SealedSectorMetadata {
    pub sector_id: SectorId,
    pub sector_access: String,
    pub pieces: Vec<PieceMetadata>,
    pub comm_r: [u8; 32],
    pub comm_d: [u8; 32],
    pub proof: Vec<u8>,
    /// checksum on the whole sector
    pub blake2b_checksum: Vec<u8>,
    /// number of bytes in the sealed sector-file as returned by `std::fs::metadata`
    pub len: u64,
    pub p_aux: PersistentAux,
    pub ticket: SealTicket,
    pub seed: SealSeed,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct PieceMetadata {
    pub piece_key: String,
    pub num_bytes: UnpaddedBytesAmount,
    pub comm_p: [u8; 32],
}

impl From<PieceMetadata> for PieceInfo {
    fn from(pm: PieceMetadata) -> Self {
        PieceInfo {
            commitment: pm.comm_p,
            size: pm.num_bytes,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct PersistablePreCommitOutput {
    pub comm_d: Commitment,
    pub comm_r: Commitment,
    pub p_aux: PersistentAux,
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum SealStatus {
    Committed(Box<SealedSectorMetadata>),
    Committing(
        SealTicket,
        TemporaryAuxKey,
        PersistablePreCommitOutput,
        SealSeed,
    ),
    CommittingPaused(
        SealTicket,
        TemporaryAuxKey,
        PersistablePreCommitOutput,
        SealSeed,
    ),
    Failed(String),
    AcceptingPieces,
    PreCommitted(SealTicket, TemporaryAuxKey, PersistablePreCommitOutput),
    PreCommitting(SealTicket),
    PreCommittingPaused(SealTicket),
    FullyPacked,
}

impl SealStatus {
    pub fn persistable_pre_commit_output(&self) -> Option<&PersistablePreCommitOutput> {
        match self {
            SealStatus::Committed(_) => None,
            SealStatus::Committing(_, _, p, _) => Some(&p),
            SealStatus::CommittingPaused(_, _, p, _) => Some(&p),
            SealStatus::Failed(_) => None,
            SealStatus::AcceptingPieces => None,
            SealStatus::PreCommitted(_, _, p) => Some(&p),
            SealStatus::PreCommitting(_) => None,
            SealStatus::PreCommittingPaused(_) => None,
            SealStatus::FullyPacked => None,
        }
    }

    pub fn ticket(&self) -> Option<&SealTicket> {
        match self {
            SealStatus::Committed(meta) => Some(&meta.ticket),
            SealStatus::Committing(t, _, _, _) => Some(&t),
            SealStatus::CommittingPaused(t, _, _, _) => Some(&t),
            SealStatus::Failed(_) => None,
            SealStatus::AcceptingPieces => None,
            SealStatus::PreCommitted(t, _, _) => Some(&t),
            SealStatus::PreCommitting(t) => Some(&t),
            SealStatus::PreCommittingPaused(t) => Some(&t),
            SealStatus::FullyPacked => None,
        }
    }

    pub fn seed(&self) -> Option<&SealSeed> {
        match self {
            SealStatus::Committed(meta) => Some(&meta.seed),
            SealStatus::Committing(_, _, _, s) => Some(&s),
            SealStatus::CommittingPaused(_, _, _, s) => Some(&s),
            SealStatus::Failed(_) => None,
            SealStatus::AcceptingPieces => None,
            SealStatus::PreCommitted(_, _, _) => None,
            SealStatus::PreCommitting(_) => None,
            SealStatus::PreCommittingPaused(_) => None,
            SealStatus::FullyPacked => None,
        }
    }
}

#[derive(Clone, Serialize, Default, Deserialize, Debug, PartialEq)]
pub struct SealTicket {
    /// the height at which we chose the ticket
    pub block_height: u64,

    /// bytes of the minimum ticket chosen from a block with given height
    pub ticket_bytes: [u8; 32],
}

#[derive(Clone, Serialize, Default, Deserialize, Debug, PartialEq)]
pub struct SealSeed {
    /// the height at which we chose the ticket
    pub block_height: u64,

    /// bytes of the minimum ticket chosen from a block with given height
    pub ticket_bytes: [u8; 32],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SealedSectorHealth {
    Ok,
    ErrorInvalidChecksum,
    ErrorInvalidLength,
    ErrorMissing,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GetSealedSectorResult {
    WithHealth(SealedSectorHealth, SealedSectorMetadata),
    WithoutHealth(SealedSectorMetadata),
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct SecondsSinceEpoch(pub u64);

impl Default for StagedSectorMetadata {
    fn default() -> StagedSectorMetadata {
        StagedSectorMetadata {
            sector_id: Default::default(),
            sector_access: Default::default(),
            pieces: Default::default(),
            seal_status: SealStatus::AcceptingPieces,
        }
    }
}
