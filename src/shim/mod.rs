// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod address;
pub mod bigint;
pub mod clock;
pub mod crypto;
pub mod deal;
pub mod econ;
pub mod error;
pub mod executor;
pub mod externs;
pub mod gas;
pub mod machine;
pub mod message;
pub mod piece;
pub mod randomness;
pub mod sector;
pub mod state_tree;
pub mod version;

///
/// Helper trait to re-use static methods and constants.
/// The usage is awkward but it avoids code duplication.
///
/// ```
/// # use forest_filecoin::doctest_private::{Inner, ExitCode};
/// <ExitCode as Inner>::FVM::new(0);
/// ```
pub trait Inner {
    type FVM;
}
