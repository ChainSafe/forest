// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod de;
mod error;
mod path;
mod path_segment;
pub mod selector;
mod ser;
pub mod util;

#[cfg(feature = "json")]
pub mod json;

#[macro_use]
mod macros;

pub use self::error::Error;
pub use path::Path;
pub use path_segment::PathSegment;
pub use util::*;

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
    /// ```no_run
    /// # use forest_ipld::ipld;
    /// let v = ipld!(null);
    /// ```
    Null,

    /// Represents a boolean value.
    ///
    /// ```no_run
    /// # use forest_ipld::ipld;
    /// let v = ipld!(true);
    /// ```
    Bool(bool),

    /// Represents a signed integer value.
    ///
    /// ```no_run
    /// # use forest_ipld::ipld;
    /// let v = ipld!(28);
    /// ```
    Integer(i128),

    /// Represents a floating point value.
    ///
    /// ```no_run
    /// # use forest_ipld::ipld;
    /// let v = ipld!(8.5);
    /// ```
    Float(f64),

    /// Represents a String.
    ///
    /// ```no_run
    /// # use forest_ipld::ipld;
    /// let v = ipld!("string");
    /// ```
    String(String),

    /// Represents Bytes.
    ///
    /// ```no_run
    /// # use forest_ipld::ipld;
    /// let v = ipld!(Bytes(vec![0x98, 0x8, 0x2a, 0xff]));
    /// ```
    Bytes(Vec<u8>),

    /// Represents List of IPLD objects.
    ///
    /// ```no_run
    /// # use forest_ipld::ipld;
    /// let v = ipld!([1, "string", null]);
    /// ```
    List(Vec<Ipld>),

    /// Represents a map of strings to Ipld objects.
    ///
    /// ```no_run
    /// # use forest_ipld::ipld;
    /// let v = ipld!({"key": "value", "bool": true});
    /// ```
    Map(BTreeMap<String, Ipld>),

    /// Represents a link to another piece of data through a content identifier (`Cid`).
    /// Using `ipld` macro, can wrap Cid with Link to be explicit of Link type, or let it resolve.
    ///
    /// ```
    /// # use forest_ipld::ipld;
    /// # use cid::Cid;
    /// let cid: Cid = "QmdfTbBqBPQ7VNxZEYEj14VmRuZBkqFbiwReogJgS1zR1n".parse().unwrap();
    /// let v1 = ipld!(Link(cid.clone()));
    /// let v2 = ipld!(cid);
    /// assert_eq!(v1, v2);
    /// ```
    Link(Cid),
}

impl Ipld {
    pub(crate) fn lookup_segment(&self, segment: &PathSegment) -> Option<&Self> {
        match self {
            Self::Map(map) => match segment {
                PathSegment::String(s) => map.get(s),
                PathSegment::Int(i) => map.get(&i.to_string()),
            },
            Self::List(list) => list.get(segment.to_index()?),
            _ => None,
        }
    }
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
