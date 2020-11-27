// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Cid;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

/// Wrapper for serializing and deserializing a Cid from JSON.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(transparent)]
pub struct CidJson(#[serde(with = "self")] pub Cid);

/// Wrapper for serializing a cid reference to JSON.
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

/// Struct just used as a helper to serialize a cid into a map with key "/"
#[derive(Serialize, Deserialize)]
struct CidMap {
    #[serde(rename = "/")]
    cid: String,
}

pub mod vec {
    use super::*;
    use forest_json_utils::GoVecVisitor;
    use serde::ser::SerializeSeq;

    /// Wrapper for serializing and deserializing a Cid vector from JSON.
    #[derive(Deserialize, Serialize)]
    #[serde(transparent)]
    pub struct CidJsonVec(#[serde(with = "self")] pub Vec<Cid>);

    /// Wrapper for serializing a cid slice to JSON.
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

pub mod opt {
    use super::{Cid, CidJson, CidJsonRef};
    use serde::{self, Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(v: &Option<Cid>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        v.as_ref().map(|s| CidJsonRef(s)).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Cid>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: Option<CidJson> = Deserialize::deserialize(deserializer)?;
        Ok(s.map(|v| v.0))
    }
}
