use std::sync::Arc;

use filecoin_proofs::types::UnpaddedBytesAmount;
use filecoin_proofs::{seal as seal_internal, SealOutput};

use crate::error;
use crate::metadata::{
    sector_id_as_bytes, PieceMetadata, SealedSectorMetadata, StagedSectorMetadata,
};
use crate::store::SectorStore;

pub fn seal(
    sector_store: &Arc<impl SectorStore>,
    prover_id: &[u8; 31],
    staged_sector: StagedSectorMetadata,
) -> error::Result<SealedSectorMetadata> {
    // Provision a new sealed sector access through the manager.
    let sealed_sector_access = sector_store
        .manager()
        .new_sealed_sector_access()
        .map_err(failure::Error::from)?;

    // Run the FPS seal operation. This call will block for a long time, so make
    // sure you're not holding any locks.

    let piece_lengths: Vec<UnpaddedBytesAmount> =
        staged_sector.pieces.iter().map(|p| p.num_bytes).collect();

    let SealOutput {
        comm_r,
        comm_d,
        comm_r_star,
        proof,
        comm_ps,
        piece_inclusion_proofs,
    } = seal_internal(
        (*sector_store).proofs_config().porep_config(),
        sector_store
            .manager()
            .staged_sector_path(&staged_sector.sector_access),
        sector_store
            .manager()
            .sealed_sector_path(&sealed_sector_access),
        prover_id,
        &sector_id_as_bytes(staged_sector.sector_id)?,
        &piece_lengths,
    )?;

    let pieces = staged_sector
        .pieces
        .into_iter()
        .zip(comm_ps.iter())
        .zip(piece_inclusion_proofs.into_iter())
        .map(|((piece, &comm_p), piece_inclusion_proof)| PieceMetadata {
            piece_key: piece.piece_key,
            num_bytes: piece.num_bytes,
            comm_p: Some(comm_p),
            piece_inclusion_proof: Some(piece_inclusion_proof.into()),
        })
        .collect();

    let newly_sealed_sector = SealedSectorMetadata {
        sector_id: staged_sector.sector_id,
        sector_access: sealed_sector_access,
        pieces,
        comm_r_star,
        comm_r,
        comm_d,
        proof,
    };

    Ok(newly_sealed_sector)
}
