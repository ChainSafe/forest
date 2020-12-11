use cid::Cid;

/// Specifies the version of the state tree
#[derive(Debug, PartialEq, Clone, Copy, PartialOrd)]
#[repr(u64)]
pub enum StateTreeVersion {
    /// Corresponds to actors < v2
    V0,
    /// Corresponds to actors >= v2
    V1,
}

pub struct StateRoot {
    /// State tree version
    pub version: StateTreeVersion,

    /// Actors tree. The structure depends on the state root version.
    pub actors: Cid,

    /// Info. The structure depends on the state root version.
    pub info: Cid,
}
