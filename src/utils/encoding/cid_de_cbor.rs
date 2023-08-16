// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::serde::BytesToCidVisitor;
use cid::Cid;
use core::fmt;
use serde::de::{self, DeserializeSeed, SeqAccess, Visitor};
use serde::Deserializer;

/// [`CidVec`] allows for efficient zero-copy de-serialization of `DAG_CBOR`-encoded nodes into a
/// vector of [`Cid`].
#[derive(Default)]
pub struct CidVec(Vec<Cid>);

impl CidVec {
    pub fn into_inner(self) -> Vec<Cid> {
        self.0
    }
}

/// [`FilterCids`] traverses an [`libipld_core::ipld::Ipld`] tree, appending [`Cid`]s (and only CIDs) to a single vector.
/// This is much faster than constructing an [`libipld_core::ipld::Ipld`] tree and then performing the filtering.
struct FilterCids<'a>(&'a mut Vec<Cid>);

impl<'de, 'a> DeserializeSeed<'de> for FilterCids<'a> {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct FilterCidsVisitor<'a>(&'a mut Vec<Cid>);

        impl<'de, 'a> Visitor<'de> for FilterCidsVisitor<'a> {
            type Value = ();

            fn expecting(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt.write_str("any valid IPLD kind")
            }

            // Recursively visit a map, equivalent to `filter_map` that finds all the `Ipld::Link`
            // and extracts a CID from them.
            #[inline]
            fn visit_map<V>(self, mut visitor: V) -> Result<Self::Value, V::Error>
            where
                V: de::MapAccess<'de>,
            {
                self.0.reserve(visitor.size_hint().unwrap_or(0));
                // This is where recursion happens, we unravel each [`Ipld`] till we reach all
                // the nodes.
                while visitor
                    .next_entry_seed(FilterCids(&mut Vec::new()), FilterCids(self.0))?
                    .is_some()
                {
                    // Nothing to do; inner map values have been into `vec`.
                }

                Ok(())
            }

            // Recursively visit a list, equivalent to `filter_map` that finds all the `Ipld::Link`
            // and extracts a CID from them.
            #[inline]
            fn visit_seq<A>(self, mut seq: A) -> Result<(), A::Error>
            where
                A: SeqAccess<'de>,
            {
                self.0.reserve(seq.size_hint().unwrap_or(0));
                // This is where recursion happens, we unravel each [`Ipld`] till we reach all
                // the nodes.
                while seq.next_element_seed(FilterCids(self.0))?.is_some() {
                    // Nothing to do; inner array has been appended into `vec`.
                }
                Ok(())
            }

            // "New-type" structs are only used to de-serialize CIDs.
            #[inline]
            fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: de::Deserializer<'de>,
            {
                let cid = deserializer.deserialize_bytes(BytesToCidVisitor)?;
                self.0.push(cid);

                Ok(())
            }

            // We don't care about anything else as the CIDs could only be found in "new-type"
            // structs. So we visit only lists, maps and said structs.
            #[inline]
            fn visit_str<E>(self, _value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(())
            }

            #[inline]
            fn visit_bytes<E>(self, _v: &[u8]) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(())
            }

            #[inline]
            fn visit_byte_buf<E>(self, _v: Vec<u8>) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(())
            }

            #[inline]
            fn visit_u64<E>(self, _v: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(())
            }

            #[inline]
            fn visit_i64<E>(self, _v: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(())
            }

            #[inline]
            fn visit_i128<E>(self, _v: i128) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(())
            }

            #[inline]
            fn visit_f64<E>(self, _v: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(())
            }

            #[inline]
            fn visit_bool<E>(self, _v: bool) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(())
            }

            #[inline]
            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(())
            }
        }

        deserializer.deserialize_any(FilterCidsVisitor(self.0))
    }
}

impl<'de> de::Deserialize<'de> for CidVec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let mut vec = CidVec::default();
        FilterCids(&mut vec.0).deserialize(deserializer)?;
        Ok(vec)
    }
}
