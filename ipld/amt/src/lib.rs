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

pub use self::amt::Amt;
pub use self::bitmap::BitMap;
pub use self::error::Error;
pub(crate) use self::node::Node;
pub(crate) use self::root::Root;

const MAX_INDEX_BITS: u64 = 63;
const WIDTH_BITS: u64 = 3;
const WIDTH: usize = 1 << WIDTH_BITS; // 8
const MAX_HEIGHT: u64 = MAX_INDEX_BITS / WIDTH_BITS - 1;

// Maximum index for elements in the AMT. This is currently 1^63
// (max int) because the width is 8. That means every "level" consumes 3 bits
// from the index, and 63/3 is a nice even 21
pub const MAX_INDEX: u64 = (1 << MAX_INDEX_BITS) - 1;

fn nodes_for_height(height: u64) -> u64 {
    let height_log_two = WIDTH_BITS * height;
    assert!(
        height_log_two < 64,
        "height overflow, should be checked at all entry points"
    );
    1 << height_log_two
}
