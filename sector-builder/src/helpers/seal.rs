use std::sync::Arc;

use filecoin_proofs::types::UnpaddedBytesAmount;
use filecoin_proofs::{seal as seal_internal, SealOutput};

use crate::error;
use crate::helpers::checksum::calculate_checksum;
use crate::metadata::{PieceMetadata, SealedSectorMetadata, StagedSectorMetadata};
use crate::store::SectorStore;

pub fn seal(
    sector_store: &Arc<impl SectorStore>,
    prover_id: &[u8; 31],
    staged_sector: StagedSectorMetadata,
) -> error::Result<SealedSectorMetadata> {
    // Provision a new sealed sector access through the manager.
    let sealed_sector_access = sector_store
        .manager()
        .new_sealed_sector_access(staged_sector.sector_id)
        .map_err(failure::Error::from)?;

    let sealed_sector_path = sector_store
        .manager()
        .sealed_sector_path(&sealed_sector_access);

    // Run the FPS seal operation. This call will block for a long time, so make
    // sure you're not holding any locks.

    info!("seal: start (sector_id={})", staged_sector.sector_id);

    let staged_sector_path = sector_store
        .manager()
        .staged_sector_path(&staged_sector.sector_access);

    let SealOutput {
        comm_r,
        comm_d,
        comm_r_star,
        proof,
        comm_ps,
        piece_inclusion_proofs,
    } = seal_internal(
        (*sector_store).proofs_config().porep_config(),
        staged_sector_path,
        sealed_sector_path.clone(),
        prover_id,
        staged_sector.sector_id,
        staged_sector
            .pieces
            .iter()
            .map(|p| p.num_bytes)
            .collect::<Vec<UnpaddedBytesAmount>>()
            .as_slice(),
    )?;

    info!("seal: finish (sector_id={})", staged_sector.sector_id);

    // generate checksum
    let blake2b_checksum = calculate_checksum(&sealed_sector_path)?.as_ref().to_vec();

    // get number of bytes in sealed sector-file
    let len = std::fs::metadata(&sealed_sector_path)?.len();

    // combine the piece commitment, piece inclusion proof, and other piece
    // metadata into a single struct (to be persisted to metadata store)
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
        blake2b_checksum,
        len,
    };

    Ok(newly_sealed_sector)
}
