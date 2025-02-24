//! CAR stream does not include duplicated blocks, so to reconstruct a UnixFS file,
//! data does not follow the same layout as the expected target file. To recreate
//! the file one must have the ability to read from arbitrary locations of the stream.
//!
//! The seek module achieves this by requiring the `out` writer to also be `AsyncSeek + AsyncRead`
//! so that it duplicated data is found it can read from itself.
//!
//! See example below of a CAR stream with de-duplicated nodes:
//!
//! Target file chunked, uppercase letters = link nodes, lowercase letters = data nodes
//! ```n
//! [ROOT                              ]
//! [X         ][Y         ][X         ]
//! [a][b][a][a][b][c][d][a][a][b][a][a]
//! ```
//!
//! Car stream layout, indexes represent time steps in the CAR stream read to match below.
//! ```n
//! 1     2  3  4  5  6  7
//! [ROOT][X][a][b][Y][c][d]
//! ```
//!
//! Representation of the "link stack". Replacing a node with `[-]` represents writing its data to out.
//! For example at step 4, when `[b]` is received that node is written to out and immediately consecutive
//! nodes `[a][a]` are already known so those are written too.
//!
//! ```n
//! 0 [ROOT]
//! 1 [X][Y][X]
//! 2 [a][b][a][a][Y][a][b][a][a]
//! 3 [-][b][a][a][Y][a][b][a][a]
//! 4 [-][-][-][-][Y][a][b][a][a]
//! 5 [-][-][-][-][-][c][d][a][a][b][a][a]
//! 6 [-][-][-][-][-][-][d][a][a][b][a][a]
//! 7 [-][-][-][-][-][-][-][-][-][-][-][-]
//! ```
//!
//! # Usage
//!
//! - To read a single file buffering the block dag [`read_single_file_buffer`]
//! - To read a single file without buffering the block dag [`read_single_file_seek`]

mod error;
mod single_file_buffer;
mod single_file_seek;
mod util;

pub use error::ReadSingleFileError;
pub use single_file_buffer::read_single_file_buffer;
pub use single_file_seek::read_single_file_seek;
