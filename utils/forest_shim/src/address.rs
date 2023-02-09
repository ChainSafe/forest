// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    fmt::Display,
    ops::{Deref, DerefMut},
    str::FromStr,
};

use fvm_shared::address::Address as Address_v2;
use fvm_shared3::address::Address as Address_v3;
pub use fvm_shared3::address::{Error, Payload, Protocol, BLS_PUB_LEN};
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

impl Address {
    pub const fn new_id(id: u64) -> Self {
        Address(Address_v3::new_id(id))
    }

    pub fn new_secp256k1(pubkey: &[u8]) -> Result<Self, Error> {
        todo!()
    }

    pub fn new_bls(pubkey: &[u8]) -> Result<Self, Error> {
        todo!()
    }

    pub fn new_actor(data: &[u8]) -> Self {
        todo!()
    }
}

impl FromStr for Address {
    type Err = Error;
    fn from_str(addr: &str) -> Result<Self, Error> {
        todo!()
    }
}
