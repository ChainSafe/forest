// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod builtin;

use serde::{Deserialize, Serialize};

pub use self::builtin::*;

#[derive(Serialize, Deserialize, PartialEq, Clone)]
pub enum Policy {
    V9(fil_actors_runtime_v9::runtime::Policy),
    V10(fil_actors_runtime_v10::runtime::Policy),
}

impl Policy {
    pub fn chain_finality(&self) -> i64 {
        match &self {
            Policy::V9(policy) => policy.chain_finality,
            Policy::V10(policy) => policy.chain_finality,
        }
    }
}

impl TryFrom<Policy> for fil_actors_runtime_v9::runtime::Policy {
    type Error = anyhow::Error;

    fn try_from(value: Policy) -> Result<Self, Self::Error> {
        match value {
            Policy::V9(policy) => Ok(policy),
            Policy::V10(_) => Err(anyhow::Error::msg("wrong policy version")),
        }
    }
}

impl TryFrom<Policy> for fil_actors_runtime_v10::runtime::Policy {
    type Error = anyhow::Error;

    fn try_from(value: Policy) -> Result<Self, Self::Error> {
        match value {
            Policy::V9(_) => Err(anyhow::Error::msg("wrong policy version")),
            Policy::V10(policy) => Ok(policy),
        }
    }
}
