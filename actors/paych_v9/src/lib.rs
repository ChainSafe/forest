// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared::error::ExitCode;
use fvm_shared::METHOD_CONSTRUCTOR;
use num_derive::FromPrimitive;

pub use self::state::{LaneState, Merge, State};
pub use self::types::*;

mod state;
pub mod testing;
mod types;

// * Updated to specs-actors commit: f47f461b0588e9f0c20c999f6f129c85d669a7aa (v3.0.2)

/// Payment Channel actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    UpdateChannelState = 2,
    Settle = 3,
    Collect = 4,
}

pub const ERR_CHANNEL_STATE_UPDATE_AFTER_SETTLED: ExitCode = ExitCode::new(32);
