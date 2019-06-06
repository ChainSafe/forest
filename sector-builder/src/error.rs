use failure::Error;

pub type Result<T> = ::std::result::Result<T, Error>;

use failure::Backtrace;
use std::fmt::Display;

#[derive(Debug, Fail)]
pub enum SectorBuilderErr {
    #[fail(
        display = "number of bytes in piece ({}) exceeds maximum ({})",
        num_bytes_in_piece, max_bytes_per_sector
    )]
    OverflowError {
        num_bytes_in_piece: u64,
        max_bytes_per_sector: u64,
    },

    #[fail(
        display = "number of bytes written ({}) does not match bytes in piece ({})",
        num_bytes_written, num_bytes_in_piece
    )]
    IncompleteWriteError {
        num_bytes_written: u64,
        num_bytes_in_piece: u64,
    },

    #[fail(display = "no piece with key {} found", _0)]
    PieceNotFound(String),

    #[fail(display = "unrecoverable error: {}", _0)]
    Unrecoverable(String, Backtrace),
}

pub fn err_piecenotfound(piece_key: String) -> SectorBuilderErr {
    SectorBuilderErr::PieceNotFound(piece_key)
}

pub fn err_unrecov<S: Display>(msg: S) -> SectorBuilderErr {
    let backtrace = failure::Backtrace::new();
    SectorBuilderErr::Unrecoverable(format!("{}", msg), backtrace)
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

#[derive(Debug, Fail)]
pub enum SectorManagerErr {
    #[fail(display = "unclassified error: {}", _0)]
    UnclassifiedError(String),

    #[fail(display = "caller error: {}", _0)]
    CallerError(String),

    #[fail(display = "receiver error: {}", _0)]
    ReceiverError(String),
}
