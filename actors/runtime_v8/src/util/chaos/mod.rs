// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared::METHOD_CONSTRUCTOR;
use num_derive::FromPrimitive;
pub use state::*;
pub use types::*;

mod state;
mod types;
mod unmarshallable;

// * Updated to test-vectors commit: 907892394dd83fe1f4bf1a82146bbbcc58963148

/// Chaos actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    CallerValidation = 2,
    CreateActor = 3,
    ResolveAddress = 4,
    DeleteActor = 5,
    Send = 6,
    MutateState = 7,
    AbortWith = 8,
    InspectRuntime = 9,
}
