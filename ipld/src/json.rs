// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::Ipld;
use serde::{de, ser, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{json, Map, Number, Value as JsonValue};
use std::collections::BTreeMap;
use std::convert::{TryFrom, TryInto};

/// Wrapper for serializing and deserializing a Ipld from JSON.
#[derive(Deserialize, Serialize)]
#[serde(transparent)]
pub struct IpldJson(#[serde(with = "self")] pub Ipld);

/// Wrapper for serializing a ipld reference to JSON.
#[derive(Serialize)]
#[serde(transparent)]
pub struct IpldJsonRef<'a>(#[serde(with = "self")] pub &'a Ipld);

// TODO serialize and deserialize should not have to go through a Json value buffer
// (unnecessary clones and copies) but the efficiency for JSON shouldn't matter too much

pub fn serialize<S>(ipld: &Ipld, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    JsonValue::try_from(ipld)
        .map_err(ser::Error::custom)?
        .serialize(serializer)
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<Ipld, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(JsonValue::deserialize(deserializer)?
        .try_into()
        .map_err(de::Error::custom)?)
}

impl TryFrom<&Ipld> for JsonValue {
    type Error = &'static str;

    fn try_from(ipld: &Ipld) -> Result<Self, Self::Error> {
        let val = match ipld {
            Ipld::Null => JsonValue::Null,
            Ipld::Bool(b) => JsonValue::Bool(*b),
            Ipld::Integer(i) => {
                let i = *i;
                if i < 0 {
                    let c = i64::try_from(i).map_err(|_| "Invalid precision for json number")?;
                    JsonValue::Number(c.into())
                } else {
                    let c = u64::try_from(i).map_err(|_| "Invalid precision for json number")?;
                    JsonValue::Number(c.into())
                }
            }
            Ipld::Float(f) => JsonValue::Number(
                Number::from_f64(*f).ok_or("Float does not have finite precision")?,
            ),
            Ipld::String(s) => JsonValue::String(s.clone()),
            Ipld::Bytes(bz) => json!({ "/": { "base64": base64::encode(bz) } }),
            Ipld::Link(cid) => json!({ "/": cid.to_string() }),
            Ipld::List(list) => JsonValue::Array(
                list.iter()
                    .map(JsonValue::try_from)
                    .collect::<Result<_, _>>()?,
            ),
            Ipld::Map(map) => {
                let mut new = Map::new();
                for (k, v) in map.iter() {
                    new.insert(k.to_string(), JsonValue::try_from(v)?);
                }
                JsonValue::Object(new)
            }
        };
        Ok(val)
    }
}

impl TryFrom<JsonValue> for Ipld {
    type Error = &'static str;

    fn try_from(json: JsonValue) -> Result<Self, Self::Error> {
        let val = match json {
            JsonValue::Null => Ipld::Null,
            JsonValue::Bool(b) => Ipld::Bool(b),
            JsonValue::Number(n) => {
                if let Some(v) = n.as_u64() {
                    Ipld::Integer(v.into())
                } else if let Some(v) = n.as_i64() {
                    Ipld::Integer(v.into())
                } else if let Some(v) = n.as_f64() {
                    Ipld::Float(v)
                } else {
                    // Json number can only be one of those three types
                    unreachable!()
                }
            }
            JsonValue::String(s) => Ipld::String(s),
            JsonValue::Array(values) => Ipld::List(
                values
                    .into_iter()
                    .map(Ipld::try_from)
                    .collect::<Result<_, _>>()?,
            ),
            JsonValue::Object(map) => {
                // Check for escaped values (Bytes and Cids)
                if map.len() == 1 {
                    if let Some(v) = map.get("/") {
                        match v {
                            JsonValue::String(s) => {
                                // Json block is a Cid
                                return Ok(Ipld::Link(
                                    s.parse().map_err(|_| "Failed not parse cid string")?,
                                ));
                            }
                            JsonValue::Object(obj) => {
                                // Are other bytes encoding types supported?
                                if let Some(JsonValue::String(bz)) = obj.get("base64") {
                                    return Ok(Ipld::Bytes(
                                        base64::decode(bz)
                                            .map_err(|_| "Failed to parse base64 bytes")?,
                                    ));
                                }
                            }
                            _ => (),
                        }
                    }
                }
                let mut new = BTreeMap::new();
                for (k, v) in map.into_iter() {
                    new.insert(k, Ipld::try_from(v)?);
                }
                Ipld::Map(new)
            }
        };
        Ok(val)
    }
}
