use std::fmt::Display;

pub use anyhow::{Error, Result};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SectorBuilderErr {
    #[error(
        "number of bytes in piece ({num_bytes_in_piece}) exceeds maximum ({max_bytes_per_sector})"
    )]
    OverflowError {
        num_bytes_in_piece: u64,
        max_bytes_per_sector: u64,
    },

    #[error("number of bytes written ({num_bytes_written}) does not match bytes in piece ({num_bytes_in_piece})")]
    IncompleteWriteError {
        num_bytes_written: u64,
        num_bytes_in_piece: u64,
    },

    #[error("no piece with key {0} found")]
    PieceNotFound(String),

    #[error("unrecoverable error: {0}")]
    Unrecoverable(#[source] anyhow::Error),
}

pub fn err_piecenotfound(piece_key: String) -> SectorBuilderErr {
    SectorBuilderErr::PieceNotFound(piece_key)
}

pub fn err_unrecov<S: Display>(msg: S) -> SectorBuilderErr {
    SectorBuilderErr::Unrecoverable(anyhow::format_err!("{}", msg))
}

pub fn err_overflow(num_bytes_in_piece: u64, max_bytes_per_sector: u64) -> SectorBuilderErr {
    SectorBuilderErr::OverflowError {
        num_bytes_in_piece,
        max_bytes_per_sector,
    }
}

pub fn err_inc_write(num_bytes_written: u64, num_bytes_in_piece: u64) -> SectorBuilderErr {
    SectorBuilderErr::IncompleteWriteError {
        num_bytes_written,
        num_bytes_in_piece,
    }
}

#[derive(Debug, Error)]
pub enum SectorManagerErr {
    #[error("unclassified error: {0}")]
    UnclassifiedError(String),

    #[error("caller error: {0}")]
    CallerError(String),

    #[error("receiver error: {0}")]
    ReceiverError(String),
}
