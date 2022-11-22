// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_ipld_encoding::tuple::*;

use fvm_shared::METHOD_CONSTRUCTOR;
use num_derive::FromPrimitive;

pub use self::state::{Entry, State};

mod state;
pub mod testing;

// * Updated to specs-actors commit: 845089a6d2580e46055c24415a6c32ee688e5186 (v3.0.0)

/// Cron actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    EpochTick = 2,
}

/// Constructor parameters for Cron actor, contains entries
/// of actors and methods to call on each epoch
#[derive(Default, Debug, Serialize_tuple, Deserialize_tuple)]
pub struct ConstructorParams {
    /// Entries is a set of actors (and corresponding methods) to call during EpochTick.
    pub entries: Vec<Entry>,
}
