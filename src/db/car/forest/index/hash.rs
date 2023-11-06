// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
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
///
/// See: <https://lemire.me/blog/2016/06/27/a-fast-alternative-to-the-modulo-reduction/>
pub fn ideal_slot_ix(hash: NonMaximalU64, num_buckets: NonZeroUsize) -> usize {
    // One could simply write `self.0 as usize % buckets` but that involves
    // a relatively slow division.
    // Splitting the hash into chunks and mapping them linearly to buckets is much faster.
    // On modern computers, this mapping can be done with a single multiplication
    // (the right shift is optimized away).

    // break 0..=u64::MAX into 'buckets' chunks and map each chunk to 0..len.
    // if buckets=2, 0..(u64::MAX/2) maps to 0, and (u64::MAX/2)..=u64::MAX maps to 1.
    usize::try_from((hash.get() as u128 * num_buckets.get() as u128) >> 64).unwrap()
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
