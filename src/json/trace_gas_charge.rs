// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

pub mod json {
    use crate::shim::executor::TraceGasCharge;

    use serde::{de, ser, Deserialize, Deserializer, Serialize, Serializer};

    use std::borrow::Cow;

    /// Wrapper for serializing and de-serializing a TraceGasCharge from JSON.
    #[derive(Deserialize, Serialize, Debug)]
    #[serde(transparent)]
    pub struct TraceGasChargeJson(#[serde(with = "self")] pub TraceGasCharge);

    /// Wrapper for serializing a TraceGasCharge reference to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct TraceGasChargeJsonRef<'a>(#[serde(with = "self")] pub &'a TraceGasCharge);

    impl From<TraceGasChargeJson> for TraceGasCharge {
        fn from(wrapper: TraceGasChargeJson) -> Self {
            wrapper.0
        }
    }

    impl From<TraceGasCharge> for TraceGasChargeJson {
        fn from(wrapper: TraceGasCharge) -> Self {
            TraceGasChargeJson(wrapper)
        }
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct JsonHelper {
        pub name: Cow<'static, str>,
        pub total_gas: u64,
        pub compute_gas: u64,
        pub other_gas: u64,
        pub duration_nanos: u64,
    }

    pub fn serialize<S>(gc: &TraceGasCharge, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        JsonHelper {
            name: gc.name.clone(),
            total_gas: gc.total_gas,
            compute_gas: gc.compute_gas,
            other_gas: gc.other_gas,
            duration_nanos: gc.duration_nanos,
        }.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<TraceGasCharge, D::Error>
    where
        D: Deserializer<'de>,
    {
        let gc: JsonHelper = Deserialize::deserialize(deserializer)?;
        Ok(TraceGasCharge {
            name: gc.name.clone(),
            total_gas: gc.total_gas,
            compute_gas: gc.compute_gas,
            other_gas: gc.other_gas,
            duration_nanos: gc.duration_nanos,
        })
    }

    pub mod vec {
        use crate::utils::json::GoVecVisitor;
        use serde::ser::SerializeSeq;

        use super::*;

        pub fn serialize<S>(m: &[TraceGasCharge], serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut seq = serializer.serialize_seq(Some(m.len()))?;
            for e in m {
                seq.serialize_element(&TraceGasChargeJsonRef(e))?;
            }
            seq.end()
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<TraceGasCharge>, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_any(GoVecVisitor::<TraceGasCharge, TraceGasChargeJson>::new())
        }
    }
}

#[cfg(test)]
pub mod tests {
}
