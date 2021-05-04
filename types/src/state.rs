// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use encoding::repr::*;
use encoding::tuple::*;
use serde::{Deserialize, Serialize};

/// Specifies the version of the state tree
#[derive(Debug, PartialEq, Clone, Copy, PartialOrd, Serialize_repr, Deserialize_repr)]
#[repr(u64)]
pub enum StateTreeVersion {
    /// Corresponds to actors < v2
    V0,
    /// Corresponds to actors = v2
    V1,
    /// Corresponds to actors = v3
    V2,
    /// Corresponds to actors >= v4
    V3,
}

/// State root information. Contains information about the version of the state tree,
/// the root of the tree, and a link to the information about the tree.
#[derive(Deserialize_tuple, Serialize_tuple)]
pub struct StateRoot {
    /// State tree version
    pub version: StateTreeVersion,

    /// Actors tree. The structure depends on the state root version.
    pub actors: Cid,

    /// Info. The structure depends on the state root version.
    pub info: Cid,
}

/// Empty state tree information. This is serialized as an array for future proofing.
#[derive(Default, Deserialize, Serialize)]
#[serde(transparent)]
pub struct StateInfo0([(); 0]);
