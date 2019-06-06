use std::path::PathBuf;
use std::sync::Arc;

use filecoin_proofs::{seal as seal_internal, SealOutput};

use crate::error;
use crate::metadata::{sector_id_as_bytes, SealedSectorMetadata, StagedSectorMetadata};
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

    let SealOutput {
        comm_r,
        comm_d,
        comm_r_star,
        proof,
    } = seal_internal(
        (*sector_store).proofs_config().porep_config(),
        &PathBuf::from(staged_sector.sector_access.clone()),
        &PathBuf::from(sealed_sector_access.clone()),
        prover_id,
        &sector_id_as_bytes(staged_sector.sector_id)?,
    )?;

    let newly_sealed_sector = SealedSectorMetadata {
        sector_id: staged_sector.sector_id,
        sector_access: sealed_sector_access,
        pieces: staged_sector.pieces,
        comm_r_star,
        comm_r,
        comm_d,
        proof,
    };

    Ok(newly_sealed_sector)
}
