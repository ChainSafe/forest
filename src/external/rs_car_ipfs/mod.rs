//! This module contains modified source code of https://crates.io/crates/rs-car-ipfs
//!
//! Wrapper for [rs-car](https://crates.io/crates/rs-car) to read files from IPFS trustless gateways with an async API.
//!
//! # Usage
//!
//! - To read a single file buffering the block dag [`single_file::read_single_file_buffer`]
//! - To read a single file without buffering the block dag [`single_file::read_single_file_seek`]

mod pb;
pub mod single_file;

pub use super::rs_car::Cid;
