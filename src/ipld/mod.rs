// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod selector;
pub mod util;

pub use ipld_core::ipld::Ipld;
pub use util::*;

#[cfg(test)]
mod tests {
    mod cbor_test;
    mod selector_explore;
    mod selector_gen_tests;
}
