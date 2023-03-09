// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod address;
pub mod bigint;
pub mod crypto;
pub mod econ;
pub mod error;
pub mod executor;
pub mod gas;
pub mod message;
pub mod randomness;
pub mod sector;
pub mod state_tree;
pub mod version;

use fil_actors_runtime_v9::builtin::network;
pub const EPOCHS_IN_DAY: fvm_shared::clock::ChainEpoch = network::EPOCHS_IN_DAY;

///
/// Helper trait to re-use static methods and constants.
/// The usage is awkward but it avoids code duplication.
///
/// ```
/// use forest_shim::Inner;
/// use forest_shim::error::ExitCode;
/// <ExitCode as Inner>::FVM::new(0);
/// ```
pub trait Inner {
    type FVM;
}
