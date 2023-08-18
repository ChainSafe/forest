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

pub mod vec {
    use crate::utils::json::GoVecVisitor;
    use serde::ser::SerializeSeq;

    use super::*;

    /// Wrapper for serializing and de-serializing a Cid vector from JSON.
    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct CidJsonVec(#[serde(with = "self")] pub Vec<Cid>);

    /// Wrapper for serializing a CID slice to JSON.
    #[derive(Serialize)]
    #[serde(transparent)]
    pub struct CidJsonSlice<'a>(#[serde(with = "self")] pub &'a [Cid]);

    pub fn serialize<S>(m: &[Cid], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(m.len()))?;
        for e in m {
            seq.serialize_element(&CidJsonRef(e))?;
        }
        seq.end()
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Cid>, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(GoVecVisitor::<Cid, CidJson>::new())
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
