// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{ipld, Ipld};
use multibase::Base;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;
use std::fmt;

const BYTES_JSON_KEY: &str = "bytes";

/// Wrapper for serializing and deserializing a Ipld from JSON.
#[derive(Deserialize, Serialize)]
#[serde(transparent)]
pub struct IpldJson(#[serde(with = "self")] pub Ipld);

/// Wrapper for serializing a ipld reference to JSON.
#[derive(Serialize)]
#[serde(transparent)]
pub struct IpldJsonRef<'a>(#[serde(with = "self")] pub &'a Ipld);

pub fn serialize<S>(ipld: &Ipld, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match &ipld {
        Ipld::Null => serializer.serialize_none(),
        Ipld::Bool(bool) => serializer.serialize_bool(*bool),
        Ipld::Integer(i128) => serializer.serialize_i128(*i128),
        Ipld::Float(f64) => serializer.serialize_f64(*f64),
        Ipld::String(string) => serializer.serialize_str(&string),
        Ipld::Bytes(bytes) => serialize(
            &ipld!({ "/": { BYTES_JSON_KEY: multibase::encode(Base::Base64, bytes) } }),
            serializer,
        ),
        Ipld::List(list) => {
            let wrapped = list.iter().map(|ipld| IpldJsonRef(ipld));
            serializer.collect_seq(wrapped)
        }
        Ipld::Map(map) => {
            let wrapped = map.iter().map(|(key, ipld)| (key, IpldJsonRef(ipld)));
            serializer.collect_map(wrapped)
        }
        Ipld::Link(cid) => serialize(&ipld!({ "/": cid.to_string() }), serializer),
    }
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<Ipld, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(JSONVisitor)
}

/// Json visitor for generating IPLD from JSON
struct JSONVisitor;
impl<'de> de::Visitor<'de> for JSONVisitor {
    type Value = Ipld;

    fn expecting(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_str("any valid JSON value")
    }

    #[inline]
    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_string(String::from(value))
    }

    #[inline]
    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Ipld::String(value))
    }
    #[inline]
    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_byte_buf(v.to_owned())
    }

    #[inline]
    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Ipld::Bytes(v))
    }

    #[inline]
    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Ipld::Integer(v.into()))
    }

    #[inline]
    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Ipld::Integer(v.into()))
    }

    #[inline]
    fn visit_i128<E>(self, v: i128) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Ipld::Integer(v))
    }

    #[inline]
    fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Ipld::Bool(v))
    }

    #[inline]
    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        self.visit_unit()
    }

    #[inline]
    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Ipld::Null)
    }

    #[inline]
    fn visit_seq<V>(self, mut visitor: V) -> Result<Self::Value, V::Error>
    where
        V: de::SeqAccess<'de>,
    {
        let mut vec = Vec::new();

        while let Some(IpldJson(elem)) = visitor.next_element()? {
            vec.push(elem);
        }

        Ok(Ipld::List(vec))
    }

    #[inline]
    fn visit_map<V>(self, mut visitor: V) -> Result<Self::Value, V::Error>
    where
        V: de::MapAccess<'de>,
    {
        let mut map = BTreeMap::new();

        while let Some((key, IpldJson(value))) = visitor.next_entry()? {
            map.insert(key, value);
        }

        if map.len() == 1 {
            if let Some(v) = map.get("/") {
                match v {
                    Ipld::String(s) => {
                        // { "/": ".." } Json block is a Cid
                        return Ok(Ipld::Link(s.parse().map_err(de::Error::custom)?));
                    }
                    Ipld::Map(obj) => {
                        if let Some(Ipld::String(s)) = obj.get(BYTES_JSON_KEY) {
                            // { "/": { "bytes": "<multibase>" } } Json block are bytes encoded
                            let (_, bz) = multibase::decode(s)
                                .map_err(|e| de::Error::custom(e.to_string()))?;
                            return Ok(Ipld::Bytes(bz));
                        }
                    }
                    _ => (),
                }
            }
        }

        Ok(Ipld::Map(map))
    }

    #[inline]
    fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Ipld::Float(v))
    }
}
