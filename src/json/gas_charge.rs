// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::shim::gas::Gas as ShimmedGas;
use crate::shim::gas::GasCharge;
use crate::shim::gas::GasChargeV3;
use crate::shim::gas::GasV3;

pub mod json {
    use cid::Cid;

    use std::borrow::Cow;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::*;

    /// Wrapper for serializing and de-serializing an `GasCharge` from JSON.
    #[derive(Deserialize, Serialize, Debug)]
    #[serde(transparent)]
    pub struct GasChargeJson(#[serde(with = "self")] pub GasCharge);

    impl From<GasChargeJson> for GasCharge {
        fn from(wrapper: GasChargeJson) -> Self {
            wrapper.0
        }
    }

    impl From<GasCharge> for GasChargeJson {
        fn from(gc: GasCharge) -> Self {
            GasChargeJson(gc)
        }
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct JsonHelper {
        pub name: Cow<'static, str>,
        pub compute_gas: u64,
        pub other_gas: u64,
        // TODO: maybe
        // pub elapsed: GasDuration,
    }

    pub fn serialize<S>(gc: &GasCharge, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            name: gc.0.name.clone(),
            compute_gas: gc.0.compute_gas.as_milligas(),
            other_gas: gc.0.other_gas.as_milligas(),
        }
        .serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<GasCharge, D::Error>
    where
        D: Deserializer<'de>,
    {
        let gc: JsonHelper = Deserialize::deserialize(deserializer)?;
        Ok(GasChargeV3 {
            name: gc.name.clone(),
            compute_gas: GasV3::from_milligas(gc.compute_gas.into()),
            other_gas: GasV3::from_milligas(gc.other_gas.into()),
            elapsed: Default::default(),
        }.into())
    }
}

#[cfg(test)]
pub mod tests {
    // todo!
}
