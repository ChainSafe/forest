// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod adt;
mod builtin;
mod policy;

pub use self::adt::*;
pub use self::builtin::*;
pub use self::policy::*;
pub use actorv0;
pub use actorv2;

use fil_types::{NetworkVersion, StateTreeVersion};

pub enum ActorVersion {
    V0,
    V2,
}

impl From<NetworkVersion> for ActorVersion {
    fn from(version: NetworkVersion) -> Self {
        match version {
            NetworkVersion::V0 | NetworkVersion::V1 | NetworkVersion::V2 | NetworkVersion::V3 => {
                ActorVersion::V0
            }
            _ => ActorVersion::V2,
        }
    }
}

impl From<StateTreeVersion> for ActorVersion {
    fn from(version: StateTreeVersion) -> Self {
        match version {
            StateTreeVersion::V0 => ActorVersion::V0,
            StateTreeVersion::V1 => ActorVersion::V2,
        }
    }
}
