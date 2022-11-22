// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared::METHOD_CONSTRUCTOR;
use num_derive::FromPrimitive;

pub use self::state::Allocation;
pub use self::state::Claim;
pub use self::state::State;
pub use self::types::*;

#[cfg(feature = "fil-actor")]
fil_actors_runtime::wasm_trampoline!(Actor);

pub mod expiration;
pub mod ext;
pub mod state;
pub mod testing;
pub mod types;

/// Account actor methods available
#[derive(FromPrimitive)]
#[repr(u64)]
pub enum Method {
    Constructor = METHOD_CONSTRUCTOR,
    AddVerifier = 2,
    RemoveVerifier = 3,
    AddVerifiedClient = 4,
    // UseBytes = 5,     // Deprecated
    // RestoreBytes = 6, // Deprecated
    RemoveVerifiedClientDataCap = 7,
    RemoveExpiredAllocations = 8,
    ClaimAllocations = 9,
    GetClaims = 10,
    ExtendClaimTerms = 11,
    RemoveExpiredClaims = 12,
    UniversalReceiverHook = frc42_dispatch::method_hash!("Receive"),
}
