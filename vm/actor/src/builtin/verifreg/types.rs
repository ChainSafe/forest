use address::Address;
use crate::StoragePower;
use num_bigint::biguint_ser::{BigUintDe,BigUintSer};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub struct VerifierParams  {
    pub address: Address,
    pub allowance : Datacap
}


pub type AddVerifierParams = VerifierParams;
pub type AddVerifierClientParams = VerifierParams;

pub struct Datacap(StoragePower);

impl Serialize for Datacap
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (
            BigUintSer(&self.0)
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Datacap
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (BigUintDe(datacap)) = Deserialize::deserialize(deserializer)?;
        Ok(Self(datacap))
    }
}