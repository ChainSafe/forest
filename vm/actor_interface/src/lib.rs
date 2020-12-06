mod builtin;

pub use self::builtin::*;
pub use actorv0;
pub use actorv2;

use fil_types::NetworkVersion;

pub enum ActorVersion {
    V1,
    V2,
}

impl From<NetworkVersion> for ActorVersion {
    fn from(version: NetworkVersion) -> Self {
        match version {
            NetworkVersion::V0 | NetworkVersion::V1 | NetworkVersion::V2 | NetworkVersion::V3 => {
                ActorVersion::V1
            }
            NetworkVersion::V4 | NetworkVersion::V5 | NetworkVersion::V6 | NetworkVersion::V7 => {
                ActorVersion::V1
            }
        }
    }
}
