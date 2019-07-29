use std::fs::File;
use std::iter::Iterator;
use std::sync::Arc;

use filecoin_proofs::pieces::{
    get_aligned_source, get_piece_alignment, sum_piece_bytes_with_alignment, PieceAlignment,
};
use filecoin_proofs::types::UnpaddedBytesAmount;

use crate::builder::*;
use crate::error::*;
use crate::metadata::{self, SealStatus, SecondsSinceEpoch, StagedSectorMetadata};
use crate::state::StagedState;
use crate::store::{SectorManager, SectorStore};

pub fn add_piece(
    sector_store: &Arc<impl SectorStore>,
    mut staged_state: &mut StagedState,
    piece_key: String,
    piece_bytes_amount: u64,
    piece_path: String,
    _store_until: SecondsSinceEpoch,
) -> Result<SectorId> {
    let sector_mgr = sector_store.manager();
    let sector_max = sector_store.sector_config().max_unsealed_bytes_per_sector();

    let piece_bytes_len = UnpaddedBytesAmount(piece_bytes_amount);

    let opt_dest_sector_id = {
        let candidates: Vec<StagedSectorMetadata> = staged_state
            .sectors
            .iter()
            .filter(|(_, v)| v.seal_status == SealStatus::Pending)
            .map(|(_, v)| (*v).clone())
            .collect();

        compute_destination_sector_id(&candidates, sector_max, piece_bytes_len)?
    };

    let dest_sector_id = opt_dest_sector_id
        .ok_or(())
        .or_else(|_| provision_new_staged_sector(sector_mgr, &mut staged_state))?;

    if let Some(s) = staged_state.sectors.get_mut(&dest_sector_id) {
        let piece_lengths: Vec<_> = s.pieces.iter().map(|p| p.num_bytes).collect();

        let (expected_num_bytes_written, mut chain) =
            get_aligned_source(File::open(piece_path)?, &piece_lengths, piece_bytes_len);

        sector_store
            .manager()
            .write_and_preprocess(&s.sector_access, &mut chain)
            .map_err(Into::into)
            .and_then(|num_bytes_written| {
                if num_bytes_written != expected_num_bytes_written {
                    Err(
                        err_inc_write(u64::from(num_bytes_written), u64::from(piece_bytes_len))
                            .into(),
                    )
                } else {
                    Ok(s.sector_id)
                }
            })
            .map(|sector_id| {
                s.pieces.push(metadata::PieceMetadata {
                    piece_key,
                    num_bytes: piece_bytes_len,
                    comm_p: None,
                    piece_inclusion_proof: None,
                });

                sector_id
            })
    } else {
        Err(err_unrecov("unable to retrieve sector from state-map").into())
    }
}

// Given a list of staged sectors which are accepting data, return the
// first staged sector into which the bytes will fit.
fn compute_destination_sector_id(
    candidate_sectors: &[StagedSectorMetadata],
    max_bytes_per_sector: UnpaddedBytesAmount,
    num_bytes_in_piece: UnpaddedBytesAmount,
) -> Result<Option<SectorId>> {
    if num_bytes_in_piece > max_bytes_per_sector {
        Err(err_overflow(num_bytes_in_piece.into(), max_bytes_per_sector.into()).into())
    } else {
        let mut vector = candidate_sectors.to_vec();
        vector.sort_by(|a, b| a.sector_id.cmp(&b.sector_id));

        Ok(vector
            .iter()
            .find(move |staged_sector| {
                let piece_lengths: Vec<_> =
                    staged_sector.pieces.iter().map(|p| p.num_bytes).collect();
                let preceding_piece_bytes = sum_piece_bytes_with_alignment(&piece_lengths);
                let PieceAlignment {
                    left_bytes,
                    right_bytes,
                } = get_piece_alignment(preceding_piece_bytes, num_bytes_in_piece);
                preceding_piece_bytes + left_bytes + num_bytes_in_piece + right_bytes
                    <= max_bytes_per_sector
            })
            .map(|x| x.sector_id))
    }
}

// Provisions a new staged sector and returns its sector_id. Not a pure
// function; creates a sector access (likely a file), increments the sector id
// nonce, and mutates the StagedState.
fn provision_new_staged_sector(
    sector_manager: &SectorManager,
    staged_state: &mut StagedState,
) -> Result<SectorId> {
    let sector_id = {
        let n = &mut staged_state.sector_id_nonce;
        *n += 1;
        *n
    };

    let access = sector_manager.new_staging_sector_access()?;

    let meta = StagedSectorMetadata {
        pieces: Default::default(),
        sector_access: access.clone(),
        sector_id,
        seal_status: SealStatus::Pending,
    };

    staged_state.sectors.insert(meta.sector_id, meta.clone());

    Ok(sector_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::PieceMetadata;

    #[test]
    fn test_alpha() {
        let mut sealed_sector_a: StagedSectorMetadata = Default::default();

        sealed_sector_a.pieces.push(PieceMetadata {
            piece_key: String::from("x"),
            num_bytes: UnpaddedBytesAmount(508),
            comm_p: None,
            piece_inclusion_proof: None,
        });

        sealed_sector_a.pieces.push(PieceMetadata {
            piece_key: String::from("x"),
            num_bytes: UnpaddedBytesAmount(254),
            comm_p: None,
            piece_inclusion_proof: None,
        });

        let mut sealed_sector_b: StagedSectorMetadata = Default::default();

        sealed_sector_b.pieces.push(PieceMetadata {
            piece_key: String::from("x"),
            num_bytes: UnpaddedBytesAmount(508),
            comm_p: None,
            piece_inclusion_proof: None,
        });

        let staged_sectors = vec![sealed_sector_a.clone(), sealed_sector_b.clone()];

        // piece takes up all remaining space in first sector
        match compute_destination_sector_id(
            &staged_sectors,
            UnpaddedBytesAmount(1016),
            UnpaddedBytesAmount(254),
        ) {
            Ok(Some(destination_sector_id)) => {
                assert_eq!(destination_sector_id, sealed_sector_a.sector_id)
            }
            _ => panic!("got no destination sector"),
        }

        // piece doesn't fit into the first, but does the second
        match compute_destination_sector_id(
            &staged_sectors,
            UnpaddedBytesAmount(1016),
            UnpaddedBytesAmount(508),
        ) {
            Ok(Some(destination_sector_id)) => {
                assert_eq!(destination_sector_id, sealed_sector_b.sector_id)
            }
            _ => panic!("got no destination sector"),
        }

        // piece doesn't fit into any in the list
        match compute_destination_sector_id(
            &staged_sectors,
            UnpaddedBytesAmount(1016),
            UnpaddedBytesAmount(1016),
        ) {
            Ok(None) => (),
            _ => panic!("got no destination sector"),
        }

        // piece is over max
        match compute_destination_sector_id(
            &staged_sectors,
            UnpaddedBytesAmount(1016),
            UnpaddedBytesAmount(1024),
        ) {
            Err(_) => (),
            _ => panic!("got no destination sector"),
        }
    }
}
