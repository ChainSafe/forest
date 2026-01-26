// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::actors::convert::*;
use fil_actors_shared::{
    v9::runtime::Policy as PolicyV9, v10::runtime::Policy as PolicyV10,
    v11::runtime::Policy as PolicyV11, v12::runtime::Policy as PolicyV12,
    v13::runtime::Policy as PolicyV13, v14::runtime::Policy as PolicyV14,
    v15::runtime::Policy as PolicyV15, v16::runtime::Policy as PolicyV16,
    v17::runtime::Policy as PolicyV17,
};
use serde::{Deserialize, Serialize};

/// Shim wrapper around the latest policy version with cross-version conversions.
#[derive(
    Debug,
    Clone,
    Default,
    Eq,
    PartialEq,
    Serialize,
    Deserialize,
    derive_more::Deref,
    derive_more::DerefMut,
    derive_more::From,
    derive_more::Into,
)]
#[serde(transparent)]
pub struct Policy(pub PolicyV13);

impl From<&Policy> for PolicyV9 {
    fn from(Policy(policy): &Policy) -> Self {
        from_policy_v13_to_v9(policy)
    }
}

impl From<&Policy> for PolicyV10 {
    fn from(Policy(policy): &Policy) -> Self {
        from_policy_v13_to_v10(policy)
    }
}

impl From<&Policy> for PolicyV11 {
    fn from(Policy(policy): &Policy) -> Self {
        from_policy_v13_to_v11(policy)
    }
}

impl From<&Policy> for PolicyV12 {
    fn from(Policy(policy): &Policy) -> Self {
        from_policy_v13_to_v12(policy)
    }
}

impl From<&Policy> for PolicyV13 {
    fn from(Policy(policy): &Policy) -> Self {
        policy.clone()
    }
}

impl From<&Policy> for PolicyV14 {
    fn from(Policy(policy): &Policy) -> Self {
        from_policy_v13_to_v14(policy)
    }
}

impl From<&Policy> for PolicyV15 {
    fn from(Policy(policy): &Policy) -> Self {
        from_policy_v13_to_v15(policy)
    }
}

impl From<&Policy> for PolicyV16 {
    fn from(Policy(policy): &Policy) -> Self {
        from_policy_v13_to_v16(policy)
    }
}

impl From<&Policy> for PolicyV17 {
    fn from(Policy(policy): &Policy) -> Self {
        from_policy_v13_to_v17(policy)
    }
}
