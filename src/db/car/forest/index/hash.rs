use std::num::NonZeroUsize;

use super::NonMaximalU64;
use cid::Cid;

/// Writing our own hash function defies conventional wisdom, but _in practice_,
/// there are few collisions.
///
/// See
/// <https://github.com/ChainSafe/forest/commit/cabb43d8a4e04d8444d3d6c99ef27cd84ded3eb5>
pub fn of(cid: &Cid) -> NonMaximalU64 {
    NonMaximalU64::fit(
        cid.hash()
            .digest()
            .chunks_exact(8)
            .map(<[u8; 8]>::try_from)
            .filter_map(Result::ok)
            .fold(cid.codec() ^ cid.hash().code(), |hash, chunk| {
                hash ^ u64::from_le_bytes(chunk)
            }),
    )
}

/// Desired slot for a hash with a given table length
pub fn ideal_slot_ix(hash: NonMaximalU64, num_buckets: NonZeroUsize) -> usize {
    usize::try_from((hash.get() as u128 * num_buckets.get() as u128) >> 64).unwrap()
}

/// Reverse engineer a hash which will be mapped to `ideal`
/// # Panics
/// - If `ideal` >= `num_buckets` - that index is impossible to achieve!
#[cfg(test)]
pub fn from_ideal_slot_ix(ideal: usize, num_buckets: NonZeroUsize) -> NonMaximalU64 {
    assert!(ideal < num_buckets.get());

    fn div_ceil(a: u128, b: u128) -> u64 {
        (a / b + (if a % b == 0 { 0 } else { 1 })) as u64
    }
    let min_with_bucket = div_ceil(
        (1_u128 << u64::BITS) * ideal as u128,
        num_buckets.get() as u128,
    );
    NonMaximalU64::new(min_with_bucket).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::cid::CidCborExt as _;
    use cid::multihash::{Code, MultihashDigest as _};

    quickcheck::quickcheck! {
        fn always_in_range(hash: NonMaximalU64, num_buckets: NonZeroUsize) -> bool {
            ideal_slot_ix(hash, num_buckets) < num_buckets.get()
        }
        fn backwards(ideal: usize, num_buckets: NonZeroUsize) -> () {
            let ideal = ideal % num_buckets;
            assert_eq!(ideal, ideal_slot_ix(from_ideal_slot_ix(ideal, num_buckets), num_buckets))
        }
    }

    /// hash stability tests
    #[test]
    fn snapshots() {
        for (cid, expected) in [
            (Cid::default(), 0),
            (
                Cid::from_cbor_blake2b256(&"forest").unwrap(),
                7060553106844083342,
            ),
            (
                Cid::from_cbor_blake2b256(&"lotus").unwrap(),
                10998694778601859716,
            ),
            (
                Cid::from_cbor_blake2b256(&"libp2p").unwrap(),
                15878333306608412239,
            ),
            (
                Cid::from_cbor_blake2b256(&"ChainSafe").unwrap(),
                17464860692676963753,
            ),
            (
                Cid::from_cbor_blake2b256(&"haskell").unwrap(),
                10392497608425502268,
            ),
            (Cid::new_v1(0xAB, Code::Identity.digest(&[])), 170),
            (Cid::new_v1(0xAC, Code::Identity.digest(&[1, 2, 3, 4])), 171),
            (
                Cid::new_v1(0xAD, Code::Identity.digest(&[1, 2, 3, 4, 5, 6, 7, 8])),
                578437695752307371,
            ),
        ] {
            assert_eq!(of(&cid), NonMaximalU64::new(expected).unwrap())
        }
    }
}
