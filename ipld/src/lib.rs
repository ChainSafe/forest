// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod de;
mod error;
mod ser;

#[cfg(feature = "json")]
pub mod json;

#[macro_use]
mod macros;

pub use self::error::Error;

use cid::Cid;
use encoding::{from_slice, to_vec, Cbor};
use ser::Serializer;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::BTreeMap;

/// Represents IPLD data structure used when serializing and deserializing data.
#[derive(Debug, Clone, PartialEq)]
pub enum Ipld {
    /// Represents a null value.
    ///
    /// ```
    /// # use forest_ipld::ipld;
    /// let v = ipld!(null);
    /// ```
    Null,

    /// Represents a boolean value.
    ///
    /// ```
    /// # use forest_ipld::ipld;
    /// let v = ipld!(true);
    /// ```
    Bool(bool),

    /// Represents a signed integer value.
    ///
    /// ```
    /// # use forest_ipld::ipld;
    /// let v = ipld!(28);
    /// ```
    Integer(i128),

    /// Represents a floating point value.
    ///
    /// ```
    /// # use forest_ipld::ipld;
    /// let v = ipld!(8.5);
    /// ```
    Float(f64),

    /// Represents a String.
    ///
    /// ```
    /// # use forest_ipld::ipld;
    /// let v = ipld!("string");
    /// ```
    String(String),

    /// Represents Bytes.
    ///
    /// ```
    /// # use forest_ipld::ipld;
    /// let v = ipld!(Bytes(vec![0x98, 0x8, 0x2a, 0xff]));
    /// ```
    Bytes(Vec<u8>),

    /// Represents List of IPLD objects.
    ///
    /// ```
    /// # use forest_ipld::ipld;
    /// let v = ipld!([1, "string", null]);
    /// ```
    List(Vec<Ipld>),

    /// Represents a map of strings to Ipld objects.
    ///
    /// ```
    /// # use forest_ipld::ipld;
    /// let v = ipld!({"key": "value", "bool": true});
    /// ```
    Map(BTreeMap<String, Ipld>),

    /// Represents a link to another piece of data through a content identifier (`Cid`).
    ///
    /// ```
    /// # use forest_ipld::ipld;
    /// # use cid::Cid;
    /// let v = ipld!(Link(Cid::default()));
    /// ```
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
    // TODO update to not go through byte buffer to convert
    // There is a good amount of overhead for this (having to implement serde::Deserializer)
    // for Ipld, but possible. The benefit isn't worth changing yet since if the value is not
    // passed by reference as needed by HAMT, then the values will have to be cloned.
    let buf = to_vec(value).map_err(|e| e.to_string())?;
    from_slice(buf.as_slice()).map_err(|e| e.to_string())
}
