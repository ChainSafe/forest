use filecoin_proofs_api::{
    Commitment, PieceInfo, RegisteredSealProof, SectorId, UnpaddedBytesAmount,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct StagedSectorMetadata {
    pub sector_id: SectorId,
    pub sector_access: String,
    pub registered_seal_proof: RegisteredSealProof,
    pub pieces: Vec<PieceMetadata>,
    pub seal_status: SealStatus,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct SealedSectorMetadata {
    pub sector_id: SectorId,
    pub sector_access: String,
    pub registered_seal_proof: RegisteredSealProof,
    pub pieces: Vec<PieceMetadata>,
    pub comm_r: [u8; 32],
    pub comm_d: [u8; 32],
    pub proof: Vec<u8>,
    /// checksum on the whole sector
    pub blake2b_checksum: Vec<u8>,
    /// number of bytes in the sealed sector-file as returned by `std::fs::metadata`
    pub len: u64,
    pub ticket: SealTicket,
    pub seed: SealSeed,
}

impl SealedSectorMetadata {
    pub fn from_id(sector_id: SectorId, registered_seal_proof: RegisteredSealProof) -> Self {
        Self {
            sector_id,
            sector_access: Default::default(),
            registered_seal_proof,
            pieces: Default::default(),
            comm_r: Default::default(),
            comm_d: Default::default(),
            proof: Default::default(),
            blake2b_checksum: Default::default(),
            len: Default::default(),
            ticket: Default::default(),
            seed: Default::default(),
        }
    }
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
    pub registered_proof: RegisteredSealProof,
    pub comm_d: Commitment,
    pub comm_r: Commitment,
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub enum SealStatus {
    Committed(Box<SealedSectorMetadata>),
    Committing(SealTicket, PersistablePreCommitOutput, SealSeed),
    CommittingPaused(SealTicket, PersistablePreCommitOutput, SealSeed),
    Failed(String),
    AcceptingPieces,
    PreCommitted(SealTicket, PersistablePreCommitOutput),
    PreCommitting(SealTicket),
    PreCommittingPaused(SealTicket),
    FullyPacked,
}

impl SealStatus {
    pub fn persistable_pre_commit_output(&self) -> Option<&PersistablePreCommitOutput> {
        match self {
            SealStatus::Committed(_) => None,
            SealStatus::Committing(_, p, _) => Some(&p),
            SealStatus::CommittingPaused(_, p, _) => Some(&p),
            SealStatus::Failed(_) => None,
            SealStatus::AcceptingPieces => None,
            SealStatus::PreCommitted(_, p) => Some(&p),
            SealStatus::PreCommitting(_) => None,
            SealStatus::PreCommittingPaused(_) => None,
            SealStatus::FullyPacked => None,
        }
    }

    pub fn ticket(&self) -> Option<&SealTicket> {
        match self {
            SealStatus::Committed(meta) => Some(&meta.ticket),
            SealStatus::Committing(t, _, _) => Some(&t),
            SealStatus::CommittingPaused(t, _, _) => Some(&t),
            SealStatus::Failed(_) => None,
            SealStatus::AcceptingPieces => None,
            SealStatus::PreCommitted(t, _) => Some(&t),
            SealStatus::PreCommitting(t) => Some(&t),
            SealStatus::PreCommittingPaused(t) => Some(&t),
            SealStatus::FullyPacked => None,
        }
    }

    pub fn seed(&self) -> Option<&SealSeed> {
        match self {
            SealStatus::Committed(meta) => Some(&meta.seed),
            SealStatus::Committing(_, _, s) => Some(&s),
            SealStatus::CommittingPaused(_, _, s) => Some(&s),
            SealStatus::Failed(_) => None,
            SealStatus::AcceptingPieces => None,
            SealStatus::PreCommitted(_, _) => None,
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

impl StagedSectorMetadata {
    pub fn from_proof(registered_seal_proof: RegisteredSealProof) -> StagedSectorMetadata {
        StagedSectorMetadata {
            registered_seal_proof,
            sector_id: Default::default(),
            sector_access: Default::default(),
            pieces: Default::default(),
            seal_status: SealStatus::AcceptingPieces,
        }
    }
}
