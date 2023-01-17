// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use fvm_shared::address::Address as Address_v2;
use fvm_shared3::address::Address as Address_v3;
use std::ops::{Deref, DerefMut};

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

impl Into<Address_v3> for Address {
    fn into(self) -> Address_v3 {
        self.0
    }
}

impl Into<Address_v2> for Address {
    fn into(self) -> Address_v2 {
        Address_v2::from_bytes(&self.to_bytes()).unwrap()
    }
}
