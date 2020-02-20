// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! AMT crate for use as rust IPLD data structure
//!
//! Data structure reference:
//! https://github.com/ipld/specs/blob/51fab05b4fe4930d3d851d50cc1e5f1a02092deb/data-structures/vector.md

mod amt;
mod bitmap;
mod error;
mod node;
mod root;

pub use self::amt::AMT;
pub use self::bitmap::BitMap;
pub use self::error::Error;
pub(crate) use self::node::Node;
pub(crate) use self::root::Root;

const WIDTH: usize = 8;
pub const MAX_INDEX: u64 = 1 << 48;

pub(crate) fn nodes_for_height(height: u32) -> u64 {
    (WIDTH as u64).pow(height)
}
