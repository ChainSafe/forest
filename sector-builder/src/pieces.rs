use std::io::Cursor;
use std::io::Read;
use std::iter::Iterator;

use filecoin_proofs::types::{UnpaddedByteIndex, UnpaddedBytesAmount};

use crate::metadata::PieceMetadata;

pub struct PieceAlignment {
    pub left_bytes: UnpaddedBytesAmount,
    pub right_bytes: UnpaddedBytesAmount,
}

/**
 * Given a list of pieces, sum the number of bytes taken by those pieces in that order.
 */
pub fn sum_piece_bytes_with_alignment(pieces: &[PieceMetadata]) -> UnpaddedBytesAmount {
    pieces.iter().fold(UnpaddedBytesAmount(0), |acc, p| {
        let PieceAlignment {
            left_bytes,
            right_bytes,
        } = get_piece_alignment(acc, p.num_bytes);

        acc + left_bytes + p.num_bytes + right_bytes
    })
}

/**
 * Given a list of pieces, find the piece with a matching piece_key.
 */
pub fn get_piece_by_key(pieces: &[PieceMetadata], piece_key: &str) -> Option<PieceMetadata> {
    pieces
        .iter()
        .find(|p| p.piece_key == piece_key)
        .map(PieceMetadata::clone)
}

/**
 * Given a list of pieces, find the byte where a given piece does or would start.
 */
pub fn get_piece_start_byte(pieces: &[PieceMetadata], piece: &PieceMetadata) -> UnpaddedByteIndex {
    // get pieces up to the target piece, or all of them if the piece doesn't exist
    let pieces: Vec<PieceMetadata> = pieces
        .iter()
        .take_while(|p| p.piece_key != piece.piece_key)
        .map(PieceMetadata::clone)
        .collect();
    // sum up all the bytes taken by the ordered pieces
    let last_byte = sum_piece_bytes_with_alignment(&pieces);
    let alignment = get_piece_alignment(last_byte, piece.num_bytes);

    // add only the left padding of the target piece to give the start of that piece's data
    UnpaddedByteIndex::from(last_byte + alignment.left_bytes)
}

/**
 * Given a number of bytes already written to a staged sector (ignoring bit padding) and a number
 * of bytes (before bit padding) to be added, return the alignment required to create a piece where
 * len(piece) == len(sector size)/(2^n) and sufficient left padding to ensure simple merkle proof
 * construction.
 */
pub fn get_piece_alignment(
    written_bytes: UnpaddedBytesAmount,
    piece_bytes: UnpaddedBytesAmount,
) -> PieceAlignment {
    // Bit padding causes bytes to only be aligned at every 127 bytes (for 31.75 bytes).
    // @TODO change this away from magic numbers.
    let minimum_piece_bytes = (4 * 32) - 1;

    let mut piece_bytes_needed = minimum_piece_bytes;

    // Calculate the next power of two multiple that will fully contain the piece's data.
    // This is required to ensure a clean piece merkle root, without being affected by
    // preceding or following pieces.
    while piece_bytes_needed < u64::from(piece_bytes) {
        piece_bytes_needed *= 2;
    }

    // Calculate the bytes being affected from the left of the piece by the previous piece.
    let encroaching = u64::from(written_bytes) % piece_bytes_needed;

    // Calculate the bytes to push from the left to ensure a clean piece merkle root.
    let left_bytes = if encroaching > 0 {
        piece_bytes_needed - encroaching
    } else {
        0
    };

    let right_bytes = piece_bytes_needed - u64::from(piece_bytes);

    PieceAlignment {
        left_bytes: UnpaddedBytesAmount(left_bytes),
        right_bytes: UnpaddedBytesAmount(right_bytes),
    }
}

/**
 * Wraps a Readable source with null bytes on either end according to a provided PieceAlignment.
 */
fn with_alignment(source: impl Read, piece_alignment: PieceAlignment) -> impl Read {
    let PieceAlignment {
        left_bytes,
        right_bytes,
    } = piece_alignment;

    let left_padding = Cursor::new(vec![0; left_bytes.into()]);
    let right_padding = Cursor::new(vec![0; right_bytes.into()]);

    left_padding.chain(source).chain(right_padding)
}

/**
 * Given an enumeration of pieces in a staged sector and a piece to be added (represented by a Read
 * and corresponding length, in UnpaddedBytesAmount) to the staged sector, produce a new Read and
 * UnpaddedBytesAmount pair which includes the appropriate amount of alignment bytes for the piece
 * to be written to the target staged sector.
 */
pub fn get_aligned_source(
    source: impl Read,
    pieces: &[PieceMetadata],
    piece_bytes: UnpaddedBytesAmount,
) -> (UnpaddedBytesAmount, impl Read) {
    let written_bytes = sum_piece_bytes_with_alignment(pieces);
    let piece_alignment = get_piece_alignment(written_bytes, piece_bytes);
    let expected_num_bytes_written =
        piece_alignment.left_bytes + piece_bytes + piece_alignment.right_bytes;

    (
        expected_num_bytes_written,
        with_alignment(source, piece_alignment),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_piece_alignment() {
        let table = vec![
            (0, 0, (0, 127)),
            (0, 127, (0, 0)),
            (0, 254, (0, 0)),
            (0, 508, (0, 0)),
            (0, 1016, (0, 0)),
            (127, 127, (0, 0)),
            (127, 254, (127, 0)),
            (127, 508, (381, 0)),
            (100, 100, (27, 27)),
            (200, 200, (54, 54)),
            (300, 300, (208, 208)),
        ];

        for (bytes_in_sector, bytes_in_piece, (expected_left_align, expected_right_align)) in
            table.clone()
        {
            let PieceAlignment {
                left_bytes: UnpaddedBytesAmount(actual_left_align),
                right_bytes: UnpaddedBytesAmount(actual_right_align),
            } = get_piece_alignment(
                UnpaddedBytesAmount(bytes_in_sector),
                UnpaddedBytesAmount(bytes_in_piece),
            );
            assert_eq!(
                (expected_left_align, expected_right_align),
                (actual_left_align, actual_right_align)
            );
        }
    }

    #[test]
    fn test_get_piece_start_byte() {
        let mut pieces: Vec<PieceMetadata> = Default::default();

        let piece_a = PieceMetadata {
            piece_key: String::from("a"),
            num_bytes: UnpaddedBytesAmount(31),
        };

        let piece_b = PieceMetadata {
            piece_key: String::from("b"),
            num_bytes: UnpaddedBytesAmount(32),
        };

        let piece_c = PieceMetadata {
            piece_key: String::from("c"),
            num_bytes: UnpaddedBytesAmount(33),
        };

        pieces.push(piece_a);
        pieces.push(piece_b);
        pieces.push(piece_c);

        assert_eq!(
            get_piece_start_byte(&pieces, &pieces[0]),
            UnpaddedByteIndex(0)
        );
        assert_eq!(
            get_piece_start_byte(&pieces, &pieces[1]),
            UnpaddedByteIndex(127)
        );
        assert_eq!(
            get_piece_start_byte(&pieces, &pieces[2]),
            UnpaddedByteIndex(254)
        );
    }
}
