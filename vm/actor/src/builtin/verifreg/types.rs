use crate::StoragePower;
use address::Address;
use num_bigint::biguint_ser::{BigUintDe, BigUintSer};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    fmt,
    ops::{Add, Sub},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifierParams {
    pub address: Address,
    pub allowance: Datacap,
}

pub type AddVerifierParams = VerifierParams;
pub type AddVerifierClientParams = VerifierParams;

pub const MINIMUM_VERIFIED_SIZE: u32 = 1 << 20;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Datacap(StoragePower);

impl Datacap {
    pub fn new(storage_power: StoragePower) -> Self {
        Self(storage_power)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BytesParams {
    pub address: Address,
    pub deal_size: Datacap,
}

pub type UseBytesParams = BytesParams;
pub type RestoreBytesParams = BytesParams;

impl Add for Datacap {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}

impl Sub for Datacap {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self(self.0 - other.0)
    }
}

impl fmt::Display for Datacap {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for Datacap {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (BigUintSer(&self.0)).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Datacap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let BigUintDe(datacap) = Deserialize::deserialize(deserializer)?;
        Ok(Self(datacap))
    }
}
