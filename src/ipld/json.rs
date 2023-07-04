// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

//! Differences between serializers
//!
//! The serializer created here uses `multihash` and `libipld-json` uses plain
//! `base64`. That means one has an extra `m` in front of all the encoded byte
//! values, using our serializer.
//!
//! For example:
//!
//! `crate::ipld`
//! `{ "/": { "bytes": "mVGhlIHF1aQ" } }`
//!
//! `libipld-json`
//! `{ "/": { "bytes": "VGhlIHF1aQ" } }`
//!
//! Since `Lotus` is also using `multihash-base64` and we're trying to be
//! compatible, we cannot switch to `libipld-json`. It may be worthwhile
//! to reconsider whether we want to stay compatible with Lotus in the future.
use std::{collections::BTreeMap, fmt};

use cid::multibase;
use libipld_macro::ipld;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use super::Ipld;

const BYTES_JSON_KEY: &str = "bytes";
const INT_JSON_KEY: &str = "int";
const FLOAT_JSON_KEY: &str = "float";

/// Wrapper for serializing and de-serializing a IPLD from JSON.
#[derive(Deserialize, Serialize)]
#[serde(transparent)]
pub struct IpldJson(#[serde(with = "self")] pub Ipld);

/// Wrapper for serializing a IPLD reference to JSON.
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
        Ipld::Integer(i128) => serialize(
            &ipld!({ "/": { INT_JSON_KEY: i128.to_string() } }),
            serializer,
        ),
        Ipld::Float(f64) => serialize(
            &ipld!({ "/": { FLOAT_JSON_KEY: f64.to_string() } }),
            serializer,
        ),
        Ipld::String(string) => serializer.serialize_str(string),
        Ipld::Bytes(bytes) => serialize(
            &ipld!({ "/": { BYTES_JSON_KEY: multibase::encode(multibase::Base::Base64, bytes) } }),
            serializer,
        ),
        Ipld::List(list) => {
            let wrapped = list.iter().map(IpldJsonRef);
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

/// JSON visitor for generating IPLD from JSON
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
                        if let Some(Ipld::String(s)) = obj.get(INT_JSON_KEY) {
                            // { "/": { "int": "i128" } }
                            let s = s
                                .parse::<i128>()
                                .map_err(|e| de::Error::custom(e.to_string()))?;
                            return Ok(Ipld::Integer(s));
                        }
                        if let Some(Ipld::String(s)) = obj.get(FLOAT_JSON_KEY) {
                            // { "/": { "float": "f64" } }
                            let s = s
                                .parse::<f64>()
                                .map_err(|e| de::Error::custom(e.to_string()))?;
                            return Ok(Ipld::Float(s));
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

#[cfg(test)]
mod tests {
    use cid::Cid;
    use quickcheck_macros::quickcheck;
    use serde_json;

    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct IpldWrapper {
        ipld: libipld_macro::Ipld,
    }

    impl quickcheck::Arbitrary for IpldWrapper {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let bool_item = bool::arbitrary(g);
            let int_item = i128::arbitrary(g);
            let float_item = f64::arbitrary(g);
            let string_item = String::arbitrary(g);
            let bytes_item = Vec::arbitrary(g);
            let list_item = Vec::new();
            let map_item = BTreeMap::new();
            let cid = Cid::new_v1(
                u64::arbitrary(g),
                cid::multihash::Multihash::wrap(u64::arbitrary(g), &[u8::arbitrary(g)]).unwrap(),
            );
            let binding = [
                Ipld::Null,
                Ipld::Bool(bool_item),
                Ipld::Integer(int_item),
                // Filter out problematic NaN values.
                Ipld::Float(if float_item.is_nan() { 0.0 } else { float_item }),
                Ipld::String(string_item),
                Ipld::Bytes(bytes_item),
                Ipld::List(list_item),
                Ipld::Map(map_item),
                Ipld::Link(cid),
            ];
            let ipld = g.choose(&binding).unwrap();
            IpldWrapper { ipld: ipld.clone() }
        }
    }

    #[quickcheck]
    fn ipld_roundtrip(ipld: IpldWrapper) {
        let serialized: String = serde_json::to_string(&IpldJsonRef(&ipld.ipld)).unwrap();
        let parsed: IpldJson = serde_json::from_str(&serialized).unwrap();
        assert_eq!(ipld.ipld, parsed.0);
    }
}
