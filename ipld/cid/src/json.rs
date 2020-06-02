// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Cid;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

/// Wrapper for serializing and deserializing a Cid from JSON.
#[derive(Deserialize, Serialize)]
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
