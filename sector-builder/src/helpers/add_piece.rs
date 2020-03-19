use std::iter::Iterator;
use std::path::Path;

use filecoin_proofs_api::{seal, RegisteredSealProof, SectorId, UnpaddedBytesAmount};

use crate::disk_backed_storage::SectorManager;
use crate::error::*;
use crate::metadata::{self, SealStatus, SecondsSinceEpoch, StagedSectorMetadata};
use crate::state::SectorBuilderState;
use crate::SectorStore;

pub async fn add_piece<U: AsRef<Path>>(
    sector_store: &SectorStore,
    mut sector_builder_state: &mut SectorBuilderState,
    registered_seal_proof: RegisteredSealProof,
    piece_bytes_amount: u64,
    piece_key: String,
    piece_path: U,
    _store_until: SecondsSinceEpoch,
) -> Result<SectorId> {
    let mgr = sector_store.manager();

    let piece_bytes_len = UnpaddedBytesAmount(piece_bytes_amount);

    // TODO: check that piece_bytes_len is less than maximum known sector size.

    let candidates: Vec<StagedSectorMetadata> = sector_builder_state
        .staged
        .sectors
        .iter()
        .filter(|(_, v)| v.seal_status == SealStatus::AcceptingPieces)
        .map(|(_, v)| (*v).clone())
        .collect();
    let opt_dest_sector_id = compute_destination_sector_id(&candidates, piece_bytes_len);

    let dest_sector_id = match opt_dest_sector_id {
        Some(id) => id,
        None => provision_new_staged_sector(mgr, registered_seal_proof, &mut sector_builder_state)
            .map_err(|err| format_err!("could not provision new staged sector: {}", err))?,
    };

    let ssm = sector_builder_state
        .staged
        .sectors
        .get(&dest_sector_id)
        .ok_or_else(|| format_err!("unable to retrieve sector from state-map"))?;

    let piece_lens_in_staged_sector_without_alignment = ssm
        .pieces
        .iter()
        .map(|p| p.num_bytes)
        .collect::<Vec<UnpaddedBytesAmount>>();

    let piece_path = piece_path.as_ref().to_path_buf();
    let staged_path = mgr.staged_sector_path(&ssm.sector_access);
    let registered_proof = ssm.registered_seal_proof;

    let (piece_info, _) = async_std::task::spawn_blocking(move || {
        let mut piece_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(piece_path)?;
        let mut staged_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(staged_path)?;

        seal::add_piece(
            registered_proof,
            &mut piece_file,
            &mut staged_file,
            piece_bytes_len,
            &piece_lens_in_staged_sector_without_alignment,
        )
    })
    .await?;

    {
        let ssm = sector_builder_state
            .staged
            .sectors
            .get_mut(&dest_sector_id)
            .ok_or_else(|| format_err!("unable to retrieve sector from state-map"))?;

        ssm.pieces.push(metadata::PieceMetadata {
            piece_key,
            comm_p: piece_info.commitment,
            num_bytes: piece_bytes_len,
        });
    }

    Ok(dest_sector_id)
}

// Given a list of staged sectors which are accepting data, return the
// first staged sector into which the bytes will fit.
fn compute_destination_sector_id(
    candidate_sectors: &[StagedSectorMetadata],
    num_bytes_in_piece: UnpaddedBytesAmount,
) -> Option<SectorId> {
    let mut vector = candidate_sectors.to_vec();
    vector.sort_by(|a, b| a.sector_id.cmp(&b.sector_id));

    vector
        .iter()
        .find(move |staged_sector| {
            let max_bytes =
                UnpaddedBytesAmount::from(staged_sector.registered_seal_proof.sector_size());
            let piece_lengths: Vec<_> = staged_sector.pieces.iter().map(|p| p.num_bytes).collect();

            let preceding_piece_bytes =
                filecoin_proofs::pieces::sum_piece_bytes_with_alignment(&piece_lengths[..]);

            let filecoin_proofs::pieces::PieceAlignment {
                left_bytes,
                right_bytes,
            } = filecoin_proofs::pieces::get_piece_alignment(
                preceding_piece_bytes,
                num_bytes_in_piece,
            );
            preceding_piece_bytes + left_bytes + num_bytes_in_piece + right_bytes <= max_bytes
        })
        .map(|s| s.sector_id)
}

pub fn acquire_new_sector_id(sector_builder_state: &mut SectorBuilderState) -> SectorId {
    let n = SectorId::from(u64::from(sector_builder_state.sector_id_nonce) + 1);
    sector_builder_state.sector_id_nonce = n;
    n
}

// Provisions a new staged sector and returns its sector_id. Not a pure
// function; creates a sector access (likely a file), increments the sector id
// nonce, and mutates the StagedState.
fn provision_new_staged_sector(
    sector_manager: &SectorManager,
    registered_seal_proof: RegisteredSealProof,
    mut sector_builder_state: &mut SectorBuilderState,
) -> Result<SectorId> {
    let sector_id = acquire_new_sector_id(&mut sector_builder_state);

    let access = sector_manager.new_staging_sector_access(sector_id)?;

    let meta = StagedSectorMetadata {
        registered_seal_proof,
        pieces: Default::default(),
        sector_access: access,
        sector_id,
        seal_status: SealStatus::AcceptingPieces,
    };

    sector_builder_state
        .staged
        .sectors
        .insert(meta.sector_id, meta);

    Ok(sector_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::PieceMetadata;

    use filecoin_proofs_api::RegisteredSealProof;

    #[test]
    fn test_alpha() {
        let mut sealed_sector_a =
            StagedSectorMetadata::from_proof(RegisteredSealProof::StackedDrg2KiBV1);

        sealed_sector_a.pieces.push(PieceMetadata {
            piece_key: String::from("x"),
            comm_p: [0u8; 32],
            num_bytes: UnpaddedBytesAmount(508),
        });

        sealed_sector_a.pieces.push(PieceMetadata {
            piece_key: String::from("x"),
            num_bytes: UnpaddedBytesAmount(254),
            comm_p: [0u8; 32],
        });

        let mut sealed_sector_b =
            StagedSectorMetadata::from_proof(RegisteredSealProof::StackedDrg2KiBV1);

        sealed_sector_b.pieces.push(PieceMetadata {
            piece_key: String::from("x"),
            num_bytes: UnpaddedBytesAmount(508),
            comm_p: [0u8; 32],
        });

        let staged_sectors = vec![sealed_sector_a.clone(), sealed_sector_b.clone()];

        // piece takes up all remaining space in first sector
        match compute_destination_sector_id(&staged_sectors, UnpaddedBytesAmount(254)) {
            Some(id) => assert_eq!(id, sealed_sector_a.sector_id),
            _ => panic!("got no destination sector"),
        }

        // piece doesn't fit into the first, but does the second
        match compute_destination_sector_id(&staged_sectors, UnpaddedBytesAmount(508)) {
            Some(id) => assert_eq!(id, sealed_sector_b.sector_id),
            _ => panic!("got no destination sector"),
        }

        // piece doesn't fit into any in the list
        match compute_destination_sector_id(&staged_sectors, UnpaddedBytesAmount(2048)) {
            None => (),
            _ => panic!("got no destination sector"),
        }
    }
}
