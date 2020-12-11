// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod builtin;
mod policy;

pub use self::builtin::*;
pub use self::policy::*;
pub use actorv0;
pub use actorv2;

use fil_types::NetworkVersion;

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
            NetworkVersion::V4 | NetworkVersion::V5 | NetworkVersion::V6 | NetworkVersion::V7 => {
                ActorVersion::V2
            }
        }
    }
}
