// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::ops::{Deref, DerefMut};

use fvm_shared::randomness::Randomness as Randomness_v2;
use fvm_shared3::randomness::Randomness as Randomness_v3;
use serde::{Deserialize, Serialize};

/// Represents a shim over `Randomness` from `fvm_shared` with convenience
/// methods to convert to an older version of the type
///
/// # Examples
/// ```
/// 
/// // Create FVM2 Randomness normally
/// let fvm2_rand = fvm_shared::randomness::Randomness(vec![]);
///
/// // Create a correspndoning FVM3 Randomness
/// let fvm3_rand = fvm_shared3::randomness::Randomness(vec![]);
///
/// // Create a shim Randomness, ensure conversions are correct
/// let rand_shim = forest_shim::randomness::Randomness::new(vec![]);
/// assert_eq!(fvm3_rand, *rand_shim);
/// assert_eq!(fvm2_rand, rand_shim.into());
/// ```
#[derive(PartialEq, Eq, Default, Clone, Debug, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Randomness(Randomness_v3);

impl Randomness {
    pub fn new(rand: Vec<u8>) -> Self {
        Randomness(Randomness_v3(rand))
    }
}

impl Deref for Randomness {
    type Target = Randomness_v3;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Randomness {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<Randomness_v3> for Randomness {
    fn from(other: Randomness_v3) -> Self {
        Randomness(other)
    }
}

impl From<Randomness_v2> for Randomness {
    fn from(other: Randomness_v2) -> Self {
        Randomness(Randomness_v3(other.0))
    }
}

impl From<Randomness> for Randomness_v3 {
    fn from(other: Randomness) -> Self {
        other.0
    }
}

impl From<Randomness> for Randomness_v2 {
    fn from(other: Randomness) -> Randomness_v2 {
        Randomness_v2(other.0 .0)
    }
}
