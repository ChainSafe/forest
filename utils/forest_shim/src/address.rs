// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    fmt::Display,
    ops::{Deref, DerefMut},
};

use fvm_shared::address::Address as Address_v2;
use fvm_shared3::address::Address as Address_v3;
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Address(Address_v3);

impl Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

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

// Conversion implementations.
// Note for `::from_bytes`. Both FVM2 and FVM3 addresses values as bytes must be
// identical and able to do a conversion, otherwise it is a logic error and
// Forest should not continue so there is no point in `TryFrom`.

impl From<Address_v3> for Address {
    fn from(other: Address_v3) -> Self {
        Address(other)
    }
}

impl From<Address_v2> for Address {
    fn from(other: Address_v2) -> Self {
        Address::from(
            Address_v3::from_bytes(&other.to_bytes())
                .expect("Couldn't convert between FVM2 and FVM3 addresses."),
        )
    }
}

impl From<&Address_v2> for Address {
    fn from(other: &Address_v2) -> Self {
        Address::from(
            Address_v3::from_bytes(&other.to_bytes())
                .expect("Couldn't convert between FVM2 and FVM3 addresses."),
        )
    }
}

impl From<Address> for Address_v3 {
    fn from(other: Address) -> Self {
        other.0
    }
}

impl From<Address> for Address_v2 {
    fn from(other: Address) -> Address_v2 {
        Address_v2::from_bytes(&other.to_bytes())
            .expect("Couldn't convert between FVM2 and FVM3 addresses")
    }
}

impl From<&Address> for Address_v2 {
    fn from(other: &Address) -> Self {
        Address_v2::from_bytes(&other.to_bytes())
            .expect("Couldn't convert between FVM2 and FVM3 addresses")
    }
}

impl From<&Address> for Address_v3 {
    fn from(other: &Address) -> Self {
        other.0
    }
}
