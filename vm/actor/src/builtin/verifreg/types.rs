// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::StoragePower;
use address::Address;
use num_bigint::biguint_ser::{BigUintDe, BigUintSer};
use num_traits::FromPrimitive;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

lazy_static! {
    pub static ref MINIMUM_VERIFIED_SIZE: StoragePower = StoragePower::from_i32(1 << 20).unwrap();// placeholder
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifierParams {
    pub address: Address,
    pub allowance: Datacap,
}

pub type AddVerifierParams = VerifierParams;
pub type AddVerifierClientParams = VerifierParams;

pub type Datacap = StoragePower;

#[derive(Clone, Debug, PartialEq)]
pub struct BytesParams {
    pub address: Address,
    pub deal_size: Datacap,
}

pub type UseBytesParams = BytesParams;
pub type RestoreBytesParams = BytesParams;

impl Serialize for VerifierParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.address, BigUintSer(&self.allowance)).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for VerifierParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (address, BigUintDe(allowance)) = Deserialize::deserialize(deserializer)?;
        Ok(Self { address, allowance })
    }
}

impl Serialize for BytesParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.address, BigUintSer(&self.deal_size)).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BytesParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (address, BigUintDe(deal_size)) = Deserialize::deserialize(deserializer)?;
        Ok(Self { address, deal_size })
    }
}
