// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
pub mod hash_map;
pub mod hash_set;
pub use hash_map::CidHashMap;
pub use hash_set::CidHashSet;
use imp::{CidV1DagCborBlake2b256, Uncompactable};

/// The core primitive for saving space in this module.
///
/// CIDs contain a significant amount of static data (such as version, codec, hash identifier, hash
/// length).
///
/// Nearly all Filecoin CIDs are `V1`,`DagCbor` encoded, and hashed with `Blake2b256` (which has a hash
/// length of 256 bits). Naively representing such a CID requires 96 bytes but the non-static portion is only
/// 32 bytes, represented as [`CidV1DagCborBlake2b256`].
///
/// In collections, choose to store only 32 bytes where possible.
///
/// Note that construction of Cids should always go through this type, to ensure
/// - canonicalisation
/// - the contract of [`Uncompactable`]
///
/// ```
/// assert_eq!(std::mem::size_of::<cid::Cid>(), 96);
/// ```
///
/// If other types of CID become popular, they should be added to this enum
#[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd)]
enum MaybeCompactedCid {
    Compact(CidV1DagCborBlake2b256),
    /// MUST NOT overlap with the above.
    Uncompactable(Uncompactable),
}

// Hide the constructors for [`Uncompactable`] and [`CidV1DagCborBlake2b256`]
mod imp {
    use super::MaybeCompactedCid;

    use cid::{
        multihash::{self, Multihash},
        Cid,
    };
    #[cfg(test)]
    use {
        crate::utils::db::CborStoreExt as _, multihash::MultihashDigest as _, quickcheck::Arbitrary,
    };

    #[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd)]
    #[repr(transparent)]
    pub struct CidV1DagCborBlake2b256 {
        digest: [u8; Self::WIDTH],
    }

    impl CidV1DagCborBlake2b256 {
        const WIDTH: usize = 32;
    }

    #[cfg(test)]
    impl Arbitrary for CidV1DagCborBlake2b256 {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            Self {
                digest: std::array::from_fn(|_ix| u8::arbitrary(g)),
            }
        }
    }

    #[test]
    fn width() {
        assert_eq!(
            multihash::Code::Blake2b256.digest(&[]).size() as usize,
            CidV1DagCborBlake2b256::WIDTH,
        );
    }

    impl TryFrom<Cid> for CidV1DagCborBlake2b256 {
        type Error = &'static str;

        fn try_from(value: Cid) -> Result<Self, Self::Error> {
            if value.version() == cid::Version::V1 && value.codec() == fvm_ipld_encoding::DAG_CBOR {
                if let Ok(small_hash) = value.hash().resize() {
                    let (code, digest, size) = small_hash.into_inner();
                    if code == u64::from(multihash::Code::Blake2b256)
                        && size as usize == Self::WIDTH
                    {
                        return Ok(Self { digest });
                    }
                }
            }
            Err("cannot be compacted")
        }
    }

    impl From<CidV1DagCborBlake2b256> for Cid {
        fn from(value: CidV1DagCborBlake2b256) -> Self {
            let CidV1DagCborBlake2b256 { digest } = value;
            Cid::new_v1(
                fvm_ipld_encoding::DAG_CBOR,
                Multihash::wrap(multihash::Code::Blake2b256.into(), digest.as_slice())
                    .expect("could not round-trip compacted CID"),
            )
        }
    }

    #[derive(Debug, Clone, Copy, Hash, Eq, PartialEq, Ord, PartialOrd)]
    #[repr(transparent)]
    pub struct Uncompactable {
        inner: Cid,
    }

    /// [`Uncompactable`] can only be created through [`MaybeCompactedCid`], since
    /// that type defines the canonical conversion
    impl From<Uncompactable> for Cid {
        fn from(value: Uncompactable) -> Self {
            value.inner
        }
    }

    impl From<Cid> for MaybeCompactedCid {
        fn from(value: Cid) -> Self {
            match value.try_into() {
                Ok(compact) => Self::Compact(compact),
                Err(_) => Self::Uncompactable(Uncompactable { inner: value }),
            }
        }
    }

    impl From<MaybeCompactedCid> for Cid {
        fn from(value: MaybeCompactedCid) -> Self {
            match value {
                MaybeCompactedCid::Compact(compact) => compact.into(),
                MaybeCompactedCid::Uncompactable(Uncompactable { inner }) => inner,
            }
        }
    }

    #[test]
    fn compactable() {
        let cid = Cid::new(
            cid::Version::V1,
            fvm_ipld_encoding::DAG_CBOR,
            multihash::Code::Blake2b256.digest("blake".as_bytes()),
        )
        .unwrap();
        assert!(matches!(cid.into(), MaybeCompactedCid::Compact(_)));
    }

    #[test]
    fn default() {
        let cid = crate::db::MemoryDB::default()
            .put_cbor_default(&())
            .unwrap();
        assert!(
            matches!(cid.into(), MaybeCompactedCid::Compact(_)),
            "the default encoding is no longer v1+dagcbor+blake2b.
            consider adding the new default CID type to [`MaybeCompactCid`]"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cid::Cid;
    use quickcheck::{quickcheck, Arbitrary};

    impl Arbitrary for MaybeCompactedCid {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            // bump the odds of a CID being compact
            let compact = MaybeCompactedCid::Compact(CidV1DagCborBlake2b256::arbitrary(g));
            let maybe_compact = Self::from(Cid::arbitrary(g));
            *g.choose(&[compact, maybe_compact]).unwrap()
        }
    }

    quickcheck! {
        fn cid_via_maybe_compacted_cid(before: Cid) -> () {
            let via = MaybeCompactedCid::from(before);
            let after = Cid::from(via);
            assert_eq!(before, after);
        }
    }
}
