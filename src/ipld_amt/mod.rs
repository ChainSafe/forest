// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
// Copyright 2021-2023 Protocol Labs
// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! AMT crate for use as rust IPLD data structure
//!
//! Data structure reference:
//! <https://github.com/ipld/specs/blob/51fab05b4fe4930d3d851d50cc1e5f1a02092deb/data-structures/vector.md>

#![allow(dead_code)]

mod amt;
mod diff;
mod error;
mod node;
mod root;
mod value_mut;

pub use self::amt::{Amt, Amtv0};
pub use self::diff::{diff, Change, ChangeType};
pub use self::error::Error;
use self::node::Node;
pub use self::value_mut::ValueMut;

const DEFAULT_BIT_WIDTH: u32 = 3;
const MAX_HEIGHT: u32 = 64;

/// `MaxIndex` is the maximum index for elements in the AMT. This `u64::MAX-1` so we
/// don't overflow `u64::MAX` when computing the length.
pub const MAX_INDEX: u64 = std::u64::MAX - 1;

fn nodes_for_height(bit_width: u32, height: u32) -> u64 {
    let height_log_two = bit_width as u64 * height as u64;
    if height_log_two >= 64 {
        return std::u64::MAX;
    }
    1 << height_log_two
}

fn init_sized_vec<V>(bit_width: u32) -> Vec<Option<V>> {
    std::iter::repeat_with(|| None)
        .take(1 << bit_width)
        .collect()
}

fn bmap_bytes(bit_width: u32) -> usize {
    if bit_width <= 3 {
        1
    } else {
        1 << (bit_width - 3)
    }
}

#[cfg(test)]
mod tests {
    mod amt_tests;
    mod diff_tests;
}
