// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::serde::BytesToCidVisitor;
use cid::Cid;
use core::fmt;
use serde::de::{self, DeserializeSeed, SeqAccess, Visitor};
use serde::Deserializer;
use std::fmt::Write;
use std::ops::{Deref, DerefMut};

/// [`CidVec`] allows for efficient zero-copy deserialization of `DAG_CBOR`-encoded nodes into a
/// vector of [`Cid`].
#[derive(Default)]
pub struct CidVec(Vec<Cid>);

impl Deref for CidVec {
    type Target = Vec<Cid>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for CidVec {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// [`CollectCid`] struct allows for recursive traversal of [`Ipld`] in order to retrieve all the
// CIDs it encounters, using a preallocated vector.
struct CollectCid<'a>(&'a mut Vec<Cid>);

impl<'de, 'a> DeserializeSeed<'de> for CollectCid<'a> {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct CollectCidVisitor<'a>(&'a mut Vec<Cid>);

        impl<'de, 'a> Visitor<'de> for CollectCidVisitor<'a> {
            type Value = ();

            fn expecting(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt.write_str("any valid IPLD kind")
            }

            #[inline]
            fn visit_str<E>(self, _value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(())
            }

            #[inline]
            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_byte_buf(v.to_owned())
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

            #[inline]
            fn visit_map<V>(self, mut visitor: V) -> Result<Self::Value, V::Error>
            where
                V: de::MapAccess<'de>,
            {
                self.0.reserve(visitor.size_hint().unwrap_or(0));
                // This is where recursion happens, we unravel each [`Ipld`] till we reach all
                // the nodes.
                while let Some(_) =
                    visitor.next_entry_seed(CollectCid(&mut Vec::new()), CollectCid(self.0))?
                {
                }

                Ok(())
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<(), A::Error>
            where
                A: SeqAccess<'de>,
            {
                self.0.reserve(seq.size_hint().unwrap_or(0));
                // This is where recursion happens, we unravel each [`Ipld`] till we reach all
                // the nodes.
                while let Some(()) = seq.next_element_seed(CollectCid(self.0))? {
                    // Nothing to do; inner array has been appended into `vec`.
                }
                Ok(())
            }

            /// Newtype structs are only used to deserialize CIDs.
            #[inline]
            fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: de::Deserializer<'de>,
            {
                let cid = deserializer.deserialize_bytes(BytesToCidVisitor)?;
                self.0.push(cid);

                Ok(())
            }
        }

        deserializer.deserialize_any(CollectCidVisitor(self.0))
    }
}

impl<'de> de::Deserialize<'de> for CidVec {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct IpldVisitor;

        impl<'de> de::Visitor<'de> for IpldVisitor {
            type Value = CidVec;

            fn expecting(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt.write_str("any valid IPLD kind")
            }

            #[inline]
            fn visit_str<E>(self, _value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Default::default())
            }

            #[inline]
            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                self.visit_byte_buf(v.to_owned())
            }

            #[inline]
            fn visit_byte_buf<E>(self, _v: Vec<u8>) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Default::default())
            }

            #[inline]
            fn visit_u64<E>(self, _v: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Default::default())
            }

            #[inline]
            fn visit_i64<E>(self, _v: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Default::default())
            }

            #[inline]
            fn visit_i128<E>(self, _v: i128) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Default::default())
            }

            #[inline]
            fn visit_f64<E>(self, _v: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Default::default())
            }

            #[inline]
            fn visit_bool<E>(self, _v: bool) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Default::default())
            }

            #[inline]
            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(Default::default())
            }

            #[inline]
            fn visit_seq<V>(self, mut visitor: V) -> Result<Self::Value, V::Error>
            where
                V: de::SeqAccess<'de>,
            {
                let mut vec = Vec::with_capacity(visitor.size_hint().unwrap_or(0));

                // This is where we delegate parsing to a recursive deserializer.
                while let Some(()) = visitor.next_element_seed(CollectCid(&mut vec))? {}

                Ok(CidVec(vec))
            }

            #[inline]
            fn visit_map<V>(self, mut visitor: V) -> Result<Self::Value, V::Error>
            where
                V: de::MapAccess<'de>,
            {
                let mut vec = Vec::with_capacity(visitor.size_hint().unwrap_or(0));

                // This is where we delegate parsing to a recursive deserializer.
                // NOTE: We are not really interested in map keys, that's essentially a noop.
                while let Some(_) =
                    visitor.next_entry_seed(CollectCid(&mut Vec::new()), CollectCid(&mut vec))?
                {
                }

                Ok(CidVec(vec))
            }

            /// Newtype structs are only used to deserialize CIDs.
            #[inline]
            fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: de::Deserializer<'de>,
            {
                let cid = deserializer.deserialize_bytes(BytesToCidVisitor)?;
                let vec = vec![cid];

                Ok(CidVec(vec))
            }
        }

        deserializer.deserialize_any(IpldVisitor)
    }
}
