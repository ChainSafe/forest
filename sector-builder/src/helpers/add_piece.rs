use std::iter::Iterator;

use filecoin_proofs::types::UnpaddedBytesAmount;

use crate::disk_backed_storage::SectorManager;
use crate::error::*;
use crate::metadata::{self, SealStatus, SecondsSinceEpoch, StagedSectorMetadata};
use crate::state::SectorBuilderState;
use crate::SectorStore;
use std::fs::OpenOptions;
use std::io::{Cursor, Read, Seek, SeekFrom};
use storage_proofs::sector::SectorId;

pub fn add_piece<U: Read>(
    sector_store: &SectorStore,
    mut sector_builder_state: &mut SectorBuilderState,
    piece_bytes_amount: u64,
    piece_key: String,
    mut piece_file: U,
    _store_until: SecondsSinceEpoch,
) -> Result<SectorId> {
    let mgr = sector_store.manager();
    let sector_max = sector_store.sector_config().max_unsealed_bytes_per_sector;

    let piece_bytes_len = UnpaddedBytesAmount(piece_bytes_amount);

    let opt_dest_sector_id = {
        let candidates: Vec<StagedSectorMetadata> = sector_builder_state
            .staged
            .sectors
            .iter()
            .filter(|(_, v)| v.seal_status == SealStatus::AcceptingPieces)
            .map(|(_, v)| (*v).clone())
            .collect();

        compute_destination_sector_id(&candidates, sector_max, piece_bytes_len)?
    };

    let dest_sector_id = opt_dest_sector_id
        .ok_or(())
        .or_else(|_| provision_new_staged_sector(mgr, &mut sector_builder_state))?;

    let ssm = sector_builder_state
        .staged
        .sectors
        .get_mut(&dest_sector_id)
        .ok_or_else(|| format_err!("unable to retrieve sector from state-map"))?;

    // TODO: Buffering the piece completely into memory is awful, but each of
    // the two function calls (add_piece and generate_piece_commitment) accept a
    // Read. Given that the piece byte-stream is represented by a Read, we can't
    // read all of its bytes in one function call and then do the same with the
    // other w/out putting the bytes into an intermediate buffer. A tee reader
    // would be appropriate here.
    let mut backing_buffer = vec![];
    let mut cursor = Cursor::new(&mut backing_buffer);

    std::io::copy(&mut piece_file, &mut cursor)
        .map_err(|err| format_err!("unable to copy piece bytes to buffer: {:?}", err))?;

    cursor
        .seek(SeekFrom::Start(0))
        .map_err(|err| format_err!("could not seek into buffer after copy: {:?}", err))?;

    let mut staged_file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(mgr.staged_sector_path(&ssm.sector_access))?;

    let piece_lens_in_staged_sector_without_alignment = ssm
        .pieces
        .iter()
        .map(|p| p.num_bytes)
        .collect::<Vec<UnpaddedBytesAmount>>();

    let total_bytes_written = filecoin_proofs::add_piece(
        &mut cursor,
        &mut staged_file,
        piece_bytes_len,
        &piece_lens_in_staged_sector_without_alignment,
    )?;

    cursor
        .seek(SeekFrom::Start(0))
        .map_err(|err| format_err!("could not seek into buffer after add_piece: {:?}", err))?;

    // sanity check to ensure we've got alignment stuff correct
    {
        let sum_piece_lens_in_sector_with_alignment =
            filecoin_proofs::pieces::sum_piece_bytes_with_alignment(
                &piece_lens_in_staged_sector_without_alignment,
            );

        let alignment_for_new_piece = filecoin_proofs::pieces::get_piece_alignment(
            sum_piece_lens_in_sector_with_alignment,
            piece_bytes_len,
        );

        assert_eq!(
            total_bytes_written,
            alignment_for_new_piece.left_bytes
                + alignment_for_new_piece.right_bytes
                + piece_bytes_len,
            "incorrect alignment bytes written to staged sector-file"
        );
    }

    let piece_info = filecoin_proofs::generate_piece_commitment(&mut cursor, piece_bytes_len)?;

    ssm.pieces.push(metadata::PieceMetadata {
        piece_key,
        comm_p: piece_info.commitment,
        num_bytes: piece_bytes_len,
    });

    Ok(ssm.sector_id)
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

                let preceding_piece_bytes =
                    filecoin_proofs::pieces::sum_piece_bytes_with_alignment(&piece_lengths);

                let filecoin_proofs::pieces::PieceAlignment {
                    left_bytes,
                    right_bytes,
                } = filecoin_proofs::pieces::get_piece_alignment(
                    preceding_piece_bytes,
                    num_bytes_in_piece,
                );
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
    sector_builder_state: &mut SectorBuilderState,
) -> Result<SectorId> {
    let sector_id = {
        let n = SectorId::from(u64::from(sector_builder_state.last_committed_sector_id) + 1);
        sector_builder_state.last_committed_sector_id = n;
        n
    };

    let access = sector_manager.new_staging_sector_access(sector_id)?;

    let meta = StagedSectorMetadata {
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

    #[test]
    fn test_alpha() {
        let mut sealed_sector_a: StagedSectorMetadata = Default::default();

        sealed_sector_a.pieces.push(PieceMetadata {
            piece_key: String::from("x"),
            num_bytes: UnpaddedBytesAmount(508),
            comm_p: [0u8; 32],
        });

        sealed_sector_a.pieces.push(PieceMetadata {
            piece_key: String::from("x"),
            num_bytes: UnpaddedBytesAmount(254),
            comm_p: [0u8; 32],
        });

        let mut sealed_sector_b: StagedSectorMetadata = Default::default();

        sealed_sector_b.pieces.push(PieceMetadata {
            piece_key: String::from("x"),
            num_bytes: UnpaddedBytesAmount(508),
            comm_p: [0u8; 32],
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
