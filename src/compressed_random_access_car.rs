//! # File format
//! - varint frame "header"
//!   [`fvm_ipld_car::CarHeader`] with `roots.len() == 0` and `version == 2`
//! - varint frame "index"
//!   an [`Index`] encoded using [`ipld_dagcbor`](serde_ipld_dagcbor)
//! - varint frame "compression extra"
//! - varint frame "compressed group"
//! - ...
//!
//! # Compressed group
//! When decompressed using the `compression_type` in [`Index`]:
//! - varint frame containing a concatenation of a [`Cid`] and its corresponding data
//! - ...

#![allow(clippy::disallowed_types)]

use bytes::Bytes;
use cid::Cid;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio_util_06::codec::{FramedRead, FramedWrite};
type VarintFrameCodec = unsigned_varint::codec::UviBytes<bytes::Bytes>;

#[repr(u64)]
pub enum CompressionType {
    None = 0,
    ZstdPerGroup = 1,
}

pub struct Encoder {
    pub blocks_per_compressed_group: u32,
    pub compression_type: CompressionType
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockLocation {
    pub compressed_group_offset_in_file: u64,
    pub index_in_group: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Index {
    pub roots: Vec<Cid>,
    pub index: Option<HashMap<Cid, BlockLocation>>,
    pub compression_type: u64,
}
