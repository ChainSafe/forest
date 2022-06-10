// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod adt;
mod builtin;
mod policy;

pub use self::adt::*;
pub use self::builtin::*;
pub use self::policy::*;
// pub use actorv0;
// pub use actorv2;
// pub use actorv3;
// pub use actorv4;
// pub use actorv5;
// pub use actorv6;
use fil_types::{NetworkVersion, StateTreeVersion};
use std::fmt::{self, Display, Formatter};

#[derive(PartialEq)]
pub enum ActorVersion {
    V0,
    V2,
    V3,
    V4,
    V5,
    V6,
}

impl Display for ActorVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::V0 => write!(f, "V0"),
            Self::V2 => write!(f, "V2"),
            Self::V3 => write!(f, "V3"),
            Self::V4 => write!(f, "V4"),
            Self::V5 => write!(f, "V5"),
            Self::V6 => write!(f, "V6"),
        }
    }
}

impl From<NetworkVersion> for ActorVersion {
    fn from(version: NetworkVersion) -> Self {
        match version {
            NetworkVersion::V0 | NetworkVersion::V1 | NetworkVersion::V2 | NetworkVersion::V3 => {
                ActorVersion::V0
            }
            NetworkVersion::V4
            | NetworkVersion::V5
            | NetworkVersion::V6
            | NetworkVersion::V7
            | NetworkVersion::V8
            | NetworkVersion::V9 => ActorVersion::V2,
            NetworkVersion::V10 | NetworkVersion::V11 => ActorVersion::V3,
            NetworkVersion::V12 => ActorVersion::V4,
            NetworkVersion::V13 => ActorVersion::V5,
            NetworkVersion::V14 => ActorVersion::V6,
            NetworkVersion::V15 => panic!("nv15 not supported by native backend"),
            _ => panic!("nv16+ not supported by native backend"),
        }
    }
}

impl From<StateTreeVersion> for ActorVersion {
    fn from(version: StateTreeVersion) -> Self {
        match version {
            StateTreeVersion::V0 => ActorVersion::V0,
            StateTreeVersion::V1 => ActorVersion::V2,
            StateTreeVersion::V2 => ActorVersion::V3,
            StateTreeVersion::V3 => ActorVersion::V6,
            StateTreeVersion::V4 => ActorVersion::V6,
            //StateTreeVersion::V4 => ActorVersion::V6, // TODO: what version should it point to?
        }
    }
}
