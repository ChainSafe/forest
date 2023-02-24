// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::{
    fmt::Display,
    ops::{Deref, DerefMut},
    str::FromStr,
};

use fvm_ipld_encoding::Cbor;
use fvm_shared::address::Address as Address_v2;
use fvm_shared3::address::Address as Address_v3;
pub use fvm_shared3::address::{
    current_network, set_current_network, Error, Network, Payload, Protocol, BLS_PUB_LEN,
};
use serde::{Deserialize, Serialize};

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Address(Address_v3);

impl Address {
    pub const fn new_id(id: u64) -> Self {
        Address(Address_v3::new_id(id))
    }

    pub fn new_actor(data: &[u8]) -> Self {
        Address(Address_v3::new_actor(data))
    }

    pub fn new_bls(pubkey: &[u8]) -> Result<Self, Error> {
        Address_v3::new_bls(pubkey).map(Address::from)
    }

    pub fn new_secp256k1(pubkey: &[u8]) -> Result<Self, Error> {
        Address_v3::new_secp256k1(pubkey).map(Address::from)
    }

    pub fn protocol(&self) -> Protocol {
        self.0.protocol()
    }

    pub fn into_payload(self) -> Payload {
        self.0.into_payload()
    }
}

impl FromStr for Address {
    type Err = <Address_v3 as FromStr>::Err;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Address_v3::from_str(s).map(Address::from)
    }
}

impl Cbor for Address {}

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
