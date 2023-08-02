// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

/// Wrapper for serializing and de-serializing a Cid from JSON.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(transparent)]
pub struct CidJson(#[serde(with = "self")] pub Cid);

/// Wrapper for serializing a CID reference to JSON.
#[derive(Serialize)]
#[serde(transparent)]
pub struct CidJsonRef<'a>(#[serde(with = "self")] pub &'a Cid);

impl From<CidJson> for Cid {
    fn from(wrapper: CidJson) -> Self {
        wrapper.0
    }
}

pub fn serialize<S>(c: &Cid, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    CidMap { cid: c.to_string() }.serialize(serializer)
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<Cid, D::Error>
where
    D: Deserializer<'de>,
{
    let CidMap { cid } = Deserialize::deserialize(deserializer)?;
    cid.parse().map_err(de::Error::custom)
}

/// Structure just used as a helper to serialize a CID into a map with key "/"
#[derive(Serialize, Deserialize)]
struct CidMap {
    #[serde(rename = "/")]
    cid: String,
}

// TODO(aatifsyed): should this even exist?
pub mod vec {
    use super::*;

    /// Wrapper for serializing and de-serializing a Cid vector from JSON.
    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct CidJsonVec(#[serde(with = "crate::json::empty_vec_is_null")] pub Vec<Cid>);
}

pub mod opt {
    use serde::{self, Deserialize, Deserializer, Serialize, Serializer};

    use super::{Cid, CidJson, CidJsonRef};

    pub fn serialize<S>(v: &Option<Cid>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        v.as_ref().map(CidJsonRef).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Cid>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: Option<CidJson> = Deserialize::deserialize(deserializer)?;
        Ok(s.map(|v| v.0))
    }
}

#[cfg(test)]
mod tests {
    use quickcheck_macros::quickcheck;
    use serde_json;

    use super::*;

    #[quickcheck]
    fn cid_roundtrip(cid: Cid) {
        let serialized = crate::to_string_with!(&cid, serialize);
        let parsed: Cid = crate::from_str_with!(&serialized, deserialize);
        assert_eq!(cid, parsed);
    }
}
