// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod de;
mod error;
pub mod selector;
mod ser;

pub use self::error::Error;

use cid::Cid;
use encoding::{from_slice, to_vec, Cbor};
use ser::Serializer;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::BTreeMap;

/// Represents IPLD data structure used when serializing and deserializing data
#[derive(Debug, Clone, PartialEq)]
pub enum Ipld {
    Null,
    Bool(bool),
    Integer(i128),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    List(Vec<Ipld>),
    Map(BTreeMap<String, Ipld>),
    Link(Cid),
}

impl Cbor for Ipld {}

/// Convert any object into an IPLD object
pub fn to_ipld<T>(ipld: T) -> Result<Ipld, Error>
where
    T: Serialize,
{
    ipld.serialize(Serializer)
}

/// Convert a `Ipld` structure into a type `T`
/// Currently converts using a byte buffer with serde_cbor
pub fn from_ipld<T>(value: &Ipld) -> Result<T, String>
where
    T: DeserializeOwned,
{
    // TODO find a way to convert without going through byte buffer
    let buf = to_vec(value).map_err(|e| e.to_string())?;
    from_slice(buf.as_slice()).map_err(|e| e.to_string())
}
