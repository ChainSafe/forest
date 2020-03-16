// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use address::Address;
use cid::Cid;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use vm::Serialized;

/// Constructor parameters
pub struct ConstructorParams {
    pub network_name: String,
}

impl Serialize for ConstructorParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        [&self.network_name].serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ConstructorParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let [network_name]: [String; 1] = Deserialize::deserialize(deserializer)?;
        Ok(Self { network_name })
    }
}

/// Exec Params
pub struct ExecParams {
    pub code_cid: Cid,
    pub constructor_params: Serialized,
}

impl Serialize for ExecParams {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.code_cid, &self.constructor_params).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ExecParams {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (code_cid, constructor_params) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            code_cid,
            constructor_params,
        })
    }
}

/// Exec Return value
pub struct ExecReturn {
    /// ID based address for created actor
    pub id_address: Address,
    /// Reorg safe address for actor
    pub robust_address: Address,
}

impl Serialize for ExecReturn {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (&self.id_address, &self.robust_address).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ExecReturn {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (id_address, robust_address) = Deserialize::deserialize(deserializer)?;
        Ok(Self {
            id_address,
            robust_address,
        })
    }
}
