// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::utils::encoding::from_slice_with_fallback;
use cid::Cid;
use cid::serde::BytesToCidVisitor;
use serde::Deserializer;
use serde::de::{self, DeserializeSeed, SeqAccess, Visitor};
use smallvec::SmallVec;
use std::fmt;

pub type SmallCidVec = SmallVec<[Cid; 8]>;

/// Find and extract all the [`Cid`] from a `DAG_CBOR`-encoded blob without employing any
/// intermediate recursive structures, eliminating unnecessary allocations.
pub fn extract_cids(cbor_blob: &[u8]) -> anyhow::Result<SmallCidVec> {
    let CidVec(v) = from_slice_with_fallback(cbor_blob)?;
    Ok(v)
}

/// [`CidVec`] allows for efficient zero-copy de-serialization of `DAG_CBOR`-encoded nodes into a
/// vector of [`Cid`].
struct CidVec(SmallCidVec);

/// [`FilterCids`] traverses an [`ipld_core::ipld::Ipld`] tree, appending [`Cid`]s (and only CIDs) to a single vector.
/// This is much faster than constructing an [`ipld_core::ipld::Ipld`] tree and then performing the filtering.
struct FilterCids<'a>(&'a mut SmallCidVec);

impl<'de> DeserializeSeed<'de> for FilterCids<'_> {
    type Value = ();

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct IgnoredSeed;

        impl<'de> DeserializeSeed<'de> for IgnoredSeed {
            type Value = ();

            fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                deserializer.deserialize_ignored_any(de::IgnoredAny)?;
                Ok(())
            }
        }

        struct FilterCidsVisitor<'a>(&'a mut SmallCidVec);

        impl<'de> Visitor<'de> for FilterCidsVisitor<'_> {
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
                let capacity = super::size_hint_cautious_cid(visitor.size_hint().unwrap_or(0));
                self.0.reserve(capacity);
                // This is where recursion happens, we unravel each [`Ipld`] till we reach all
                // the nodes.
                while visitor
                    .next_entry_seed(IgnoredSeed, FilterCids(self.0))?
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
                let capacity = super::size_hint_cautious_cid(seq.size_hint().unwrap_or(0));
                self.0.reserve(capacity);
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
        let mut vec = CidVec(SmallCidVec::new());
        FilterCids(&mut vec.0).deserialize(deserializer)?;
        Ok(vec)
    }
}

#[cfg(test)]
mod test {
    use crate::ipld::DfsIter;
    use crate::utils::encoding::extract_cids;
    use crate::utils::multihash::prelude::*;
    use cid::Cid;
    use fvm_ipld_encoding::DAG_CBOR;
    use ipld_core::ipld::Ipld;
    use quickcheck::{Arbitrary, Gen};
    use quickcheck_macros::quickcheck;

    #[derive(Debug, Clone)]
    pub struct IpldWrapper {
        inner: Ipld,
    }

    impl Arbitrary for IpldWrapper {
        fn arbitrary(g: &mut Gen) -> Self {
            let mut ipld = Ipld::arbitrary(g);

            fn cleanup_ipld(ipld: &mut Ipld, g: &mut Gen) {
                match ipld {
                    // [`Cid`]s have to be valid in order to be decodable.
                    Ipld::Link(cid) => {
                        *cid = Cid::new_v1(
                            DAG_CBOR,
                            MultihashCode::Blake2b256.digest(&[
                                u8::arbitrary(g),
                                u8::arbitrary(g),
                                u8::arbitrary(g),
                            ]),
                        )
                    }
                    Ipld::Map(map) => map.values_mut().for_each(|val| cleanup_ipld(val, g)),
                    Ipld::List(vec) => vec.iter_mut().for_each(|val| cleanup_ipld(val, g)),
                    // Cleaning up Integer and Float to avoid unwrap panics on error. We could get
                    // away with `let Ok(blob)..`, but it is going to disable a big amount of test
                    // scenarios.
                    //
                    // Note that we don't actually care about what integer or float contain for
                    // these tests, because our deserializer ignores those as it only cares about
                    // maps, lists and [`Cid`]s.
                    // See https://github.com/ipld/serde_ipld_dagcbor/commit/94777d325a4a2bd37e8941f3fa47eba321776f65#diff-02292e8d776b8c5c924b5e32f227e772514cb68b37f3f7b384f02cdb6717a181R305-R321.
                    Ipld::Integer(int) => *int = 0,
                    // See https://github.com/ipld/serde_ipld_dagcbor/blob/379581691d82a68a774f87deb9462091ec3c8cb6/src/ser.rs#L138.
                    Ipld::Float(float) => *float = 0.0,
                    _ => (),
                }
            }
            cleanup_ipld(&mut ipld, g);
            IpldWrapper { inner: ipld }
        }
    }

    #[quickcheck]
    fn deserialize_various_blobs(ipld: IpldWrapper) {
        let ipld_to_cid = |ipld| {
            if let Ipld::Link(cid) = ipld {
                Some(cid)
            } else {
                None
            }
        };

        let blob = serde_ipld_dagcbor::to_vec(&ipld.inner).unwrap();
        let cid_vec: Vec<Cid> = DfsIter::new(ipld.inner).filter_map(ipld_to_cid).collect();
        let extracted_cid_vec = extract_cids(&blob).unwrap();
        assert_eq!(extracted_cid_vec.len(), cid_vec.len());
        assert!(extracted_cid_vec.iter().all(|item| cid_vec.contains(item)));
    }
}
