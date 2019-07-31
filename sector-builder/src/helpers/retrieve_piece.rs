use std::sync::Arc;

use filecoin_proofs::get_unsealed_range;
use filecoin_proofs::pieces::get_piece_start_byte;
use filecoin_proofs::types::UnpaddedBytesAmount;

use crate::metadata::{sector_id_as_bytes, SealedSectorMetadata};
use crate::store::SectorStore;
use crate::{err_unrecov, error};

// Unseals and returns the piece-bytes for the first sector found containing
// a piece with matching key.
pub fn retrieve_piece<'a>(
    sector_store: &Arc<impl SectorStore>,
    sealed_sector: &SealedSectorMetadata,
    prover_id: &[u8; 31],
    piece_key: &'a str,
) -> error::Result<Vec<u8>> {
    let staging_sector_access = sector_store
        .manager()
        .new_staging_sector_access()
        .map_err(failure::Error::from)?;

    let result = retrieve_piece_aux(
        sector_store,
        sealed_sector,
        prover_id,
        piece_key,
        &staging_sector_access,
    );

    if result.is_ok() {
        sector_store
            .manager()
            .delete_staging_sector_access(&staging_sector_access)?;
    }

    let (_, bytes) = result?;

    Ok(bytes)
}

fn retrieve_piece_aux<'a>(
    sector_store: &Arc<impl SectorStore>,
    sealed_sector: &SealedSectorMetadata,
    prover_id: &[u8; 31],
    piece_key: &'a str,
    staged_sector_access: &'a str,
) -> error::Result<(UnpaddedBytesAmount, Vec<u8>)> {
    let piece = sealed_sector
        .pieces
        .iter()
        .find(|p| p.piece_key == piece_key)
        .ok_or_else(|| {
            let msg = format!(
                "piece {} not found in sector {}",
                piece_key, &sealed_sector.sector_id
            );
            err_unrecov(msg)
        })?;

    let piece_lengths: Vec<_> = sealed_sector
        .pieces
        .iter()
        .take_while(|p| p.piece_key != piece_key)
        .map(|p| p.num_bytes)
        .collect();

    let num_bytes_unsealed = get_unsealed_range(
        (*sector_store).proofs_config().porep_config(),
        sector_store
            .manager()
            .sealed_sector_path(&sealed_sector.sector_access),
        sector_store
            .manager()
            .staged_sector_path(staged_sector_access),
        prover_id,
        &sector_id_as_bytes(sealed_sector.sector_id)?,
        get_piece_start_byte(&piece_lengths, piece.num_bytes),
        piece.num_bytes,
    )?;

    if num_bytes_unsealed != piece.num_bytes {
        let s = format!(
            "expected to unseal {} bytes, but unsealed {} bytes",
            u64::from(piece.num_bytes),
            u64::from(num_bytes_unsealed)
        );

        return Err(err_unrecov(s).into());
    }

    let piece_bytes =
        sector_store
            .manager()
            .read_raw(staged_sector_access, 0, num_bytes_unsealed)?;

    Ok((num_bytes_unsealed, piece_bytes))
}
