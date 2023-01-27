// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared::address::Address as Address_v2;
use fvm_shared3::address::Address as Address_v3;
use serde::{Deserialize, Serialize};
use std::ops::{Deref, DerefMut};

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Address(Address_v3);

impl Deref for Address {
    type Target = Address_v3;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Address {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<Address_v3> for Address {
    fn from(other: Address_v3) -> Self {
        Address(other)
    }
}

impl From<Address_v2> for Address {
    fn from(other: Address_v2) -> Self {
        Address::from(Address_v3::from_bytes(&other.to_bytes()).unwrap())
    }
}

impl From<Address> for Address_v3 {
    fn from(other: Address) -> Self {
        other.0
    }
}

impl From<Address> for Address_v2 {
    fn from(other: Address) -> Address_v2 {
        Address_v2::from_bytes(&other.to_bytes()).unwrap()
    }
}
