// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
use cid::Cid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hash(u64);

impl Hash {
    pub const INVALID: Hash = Hash(u64::MAX);
}

impl std::ops::Not for Hash {
    type Output = Hash;
    fn not(self) -> Hash {
        Hash::from(self.0.not())
    }
}

impl From<Hash> for u64 {
    fn from(Hash(hash): Hash) -> u64 {
        hash
    }
}

impl From<u64> for Hash {
    fn from(hash: u64) -> Hash {
        // reserve u64::MAX for empty slots.
        Hash(hash.saturating_sub(1))
    }
}

impl From<Cid> for Hash {
    fn from(cid: Cid) -> Hash {
        // Don't use DefaultHasher, it is not stable over time.
        // // use std::collections::hash_map::DefaultHasher;
        // // use std::hash::Hasher;
        // // let mut hasher = DefaultHasher::new();
        // // std::hash::Hash::hash(&cid, &mut hasher);
        // // Hash::from(hasher.finish())
        cid.hash()
            .digest()
            .chunks_exact(8)
            .map(<[u8; 8]>::try_from)
            .filter_map(Result::ok)
            .fold(cid.codec() ^ cid.hash().code(), |hash, chunk| {
                hash ^ u64::from_le_bytes(chunk)
            })
            .into()
    }
}

impl Hash {
    pub fn from_le_bytes(bytes: [u8; 8]) -> Hash {
        Hash(u64::from_le_bytes(bytes))
    }

    pub fn to_le_bytes(self) -> [u8; 8] {
        self.0.to_le_bytes()
    }

    // See: https://lemire.me/blog/2016/06/27/a-fast-alternative-to-the-modulo-reduction/
    // Desired bucket for a hash with a given table length
    pub fn bucket(&self, buckets: u64) -> u64 {
        // One could simply write `self.0 as usize % buckets` but that involves
        // a division is slow (as seen in criterion benchmarks). Splitting the
        // hash into chunks and mapping them linearly to buckets is much faster.
        // On modern computers, this mapping can be done with a single
        // multiplication (the right shift is optimized away).

        // break 0..=u64::MAX into 'buckets' chunks and map each chunk to 0..len.
        // if buckets=2, 0..(u64::MAX/2) maps to 0, and (u64::MAX/2)..=u64::MAX maps to 1.
        ((self.0 as u128 * buckets as u128) >> 64) as u64
    }

    // hash.set_bucket(x, len).bucket(len) = x
    pub fn set_bucket(self, bucket: u64, buckets: u64) -> Self {
        fn div_ceil(a: u128, b: u128) -> u64 {
            (a / b + (if a % b == 0 { 0 } else { 1 })) as u64
        }
        // Smallest number in 'bucket'
        let min_with_bucket = div_ceil((1_u128 << u64::BITS) * bucket as u128, buckets as u128);
        let bucket_height = u64::MAX / buckets;
        // Pick pseudo-random number between the smallest number in the bucket
        // and the highest
        Hash((min_with_bucket + self.0 % bucket_height).min(u64::MAX - 1))
    }

    // Walking distance between `actual_bucket` and `hash.bucket()`
    pub fn distance(&self, actual_bucket: u64, buckets: u64) -> u64 {
        let bucket = self.bucket(buckets);
        if bucket > actual_bucket {
            buckets - bucket + actual_bucket
        } else {
            actual_bucket - bucket
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::cid::CidCborExt;
    use cid::multihash::{Code, MultihashDigest};
    use itertools::Itertools;
    use quickcheck::{Arbitrary, Gen};
    use quickcheck_macros::quickcheck;
    use std::num::NonZeroUsize;

    impl Arbitrary for Hash {
        fn arbitrary(g: &mut Gen) -> Hash {
            Hash::from(u64::arbitrary(g))
        }
    }

    #[quickcheck]
    fn hash_may_not_be_invalid(cid: Cid) {
        assert_ne!(Hash::from(cid), Hash::INVALID);
    }

    #[quickcheck]
    fn hash_offset_range(hash: Hash, buckets: NonZeroUsize) {
        // The optimal offset must be in 0..buckets
        assert!(hash.bucket(buckets.get() as u64) < buckets.get() as u64)
    }

    #[quickcheck]
    fn hash_roundtrip(hash: Hash) {
        assert_eq!(hash, Hash::from_le_bytes(hash.to_le_bytes()))
    }

    #[quickcheck]
    fn hash_set_bucket(hash: Hash, mut bucket: u64, mut buckets: u64) {
        buckets = buckets.saturating_add(1); // len is non-zero
        bucket %= buckets; // offset is smaller than len
        assert_eq!(bucket, hash.set_bucket(bucket, buckets).bucket(buckets))
    }

    // small offsets and lengths can be tested exhaustively
    #[quickcheck]
    fn hash_set_bucket_small(hash: Hash) {
        for buckets in 1..u8::MAX {
            for bucket in 0..buckets {
                assert_eq!(
                    bucket as u64,
                    hash.set_bucket(bucket as u64, buckets as u64)
                        .bucket(buckets as u64),
                    "failed to set offset with buckets={buckets} and bucket={bucket}"
                )
            }
        }
    }

    #[quickcheck]
    fn hash_distance_range(hash: Hash, bucket: u64, buckets: NonZeroUsize) {
        // A hash can never be more than buckets-1 steps away from its optimal offset
        assert!(
            hash.distance(bucket % buckets.get() as u64, buckets.get() as u64)
                < buckets.get() as u64
        )
    }

    #[test]
    fn hash_distance_1() {
        // Hash(0) is right where it wants to be
        assert_eq!(Hash(0).distance(0, 1), 0);
    }

    #[test]
    fn hash_distance_2() {
        // If Hash(0) is at position 4 then it is 4 places away from where it wants to be.
        assert_eq!(Hash(0).distance(4, 10), 4);
    }

    // The hashes must be static. If any of these tests fail, the index version
    // number must be bumped.
    #[test]
    fn known_hashes() {
        assert_eq!(Hash::from(Cid::default()), Hash(0));
        assert_eq!(
            Hash::from(Cid::from_cbor_blake2b256(&"forest").unwrap()),
            Hash(7060553106844083342)
        );
        assert_eq!(
            Hash::from(Cid::from_cbor_blake2b256(&"lotus").unwrap()),
            Hash(10998694778601859716)
        );
        assert_eq!(
            Hash::from(Cid::from_cbor_blake2b256(&"libp2p").unwrap()),
            Hash(15878333306608412239)
        );
        assert_eq!(
            Hash::from(Cid::from_cbor_blake2b256(&"ChainSafe").unwrap()),
            Hash(17464860692676963753)
        );
        assert_eq!(
            Hash::from(Cid::from_cbor_blake2b256(&"haskell").unwrap()),
            Hash(10392497608425502268)
        );
        assert_eq!(
            Hash::from(Cid::new_v1(0xAB, Code::Identity.digest(&[]))),
            Hash(170)
        );
        assert_eq!(
            Hash::from(Cid::new_v1(0xAC, Code::Identity.digest(&[1, 2, 3, 4]))),
            Hash(171)
        );
        assert_eq!(
            Hash::from(Cid::new_v1(
                0xAD,
                Code::Identity.digest(&[1, 2, 3, 4, 5, 6, 7, 8])
            )),
            Hash(578437695752307371)
        );
    }

    #[test]
    fn collisions() {
        quickcheck::quickcheck(Test);

        use super::Hash as OurHash;
        use ahash::AHasher;
        use hashers::{
            builtin::DefaultHasher,
            fnv::{FNV1aHasher32, FNV1aHasher64},
            fx_hash::{FxHasher32, FxHasher64},
            jenkins::{spooky_hash::SpookyHasher, Lookup3Hasher, OAATHasher},
            null::{NullHasher, PassThroughHasher},
            oz::{DJB2Hasher, LoseLoseHasher, SDBMHasher},
            pigeon::Bricolage,
        };
        use quickcheck::{Gen, TestResult, Testable};
        use siphasher::sip::{SipHasher13, SipHasher24};
        use std::{
            collections::HashMap,
            hash::{Hash, Hasher},
        };
        macro_rules! with_name {
            ($($ident:ident),* $(,)?) => {
                [$(
                    (stringify!($ident), $ident),
                )*]
            }
        }
        struct Test;
        impl Testable for Test {
            fn result(&self, g: &mut Gen) -> TestResult {
                // hasher -> hash -> count
                let mut table = HashMap::new();
                for n in 0.. {
                    let cid = Cid::arbitrary(g);
                    let ours = u64::from(OurHash::from(cid));
                    let ahash = hash_once::<AHasher>(cid);
                    let sip13 = hash_once::<SipHasher13>(cid);
                    let sip24 = hash_once::<SipHasher24>(cid);
                    let builtin = hash_once::<DefaultHasher>(cid);
                    let fnv32 = hash_once::<FNV1aHasher32>(cid);
                    let fvn64 = hash_once::<FNV1aHasher64>(cid);
                    let fx32 = hash_once::<FxHasher32>(cid);
                    let fx64 = hash_once::<FxHasher64>(cid);
                    let spooky = hash_once::<SpookyHasher>(cid);
                    let lookup3 = hash_once::<Lookup3Hasher>(cid);
                    let oaat = hash_once::<OAATHasher>(cid);
                    let null = hash_once::<NullHasher>(cid);
                    let passthrough = hash_once::<PassThroughHasher>(cid);
                    let djb2 = hash_once::<DJB2Hasher>(cid);
                    let loselose = hash_once::<LoseLoseHasher>(cid);
                    let sdbm = hash_once::<SDBMHasher>(cid);
                    let bricolage = hash_once::<Bricolage>(cid);

                    for (name, hash) in with_name![
                        ours,
                        ahash,
                        sip13,
                        sip24,
                        builtin,
                        fnv32,
                        fvn64,
                        fx32,
                        fx64,
                        spooky,
                        lookup3,
                        oaat,
                        null,
                        passthrough,
                        djb2,
                        loselose,
                        sdbm,
                        bricolage,
                    ] {
                        table
                            .entry(name)
                            .or_insert(HashMap::new())
                            .entry(hash)
                            .and_modify(|it| *it += 1)
                            .or_insert(1);
                    }

                    if n % 10000 == 0 {
                        println!("cids: {}", n);
                        for (name, ncollisions) in table
                            .iter()
                            .map(|(name, seen)| {
                                (name, seen.values().filter(|it| **it > 1).sum::<u32>())
                            })
                            .sorted_by_key(|(_, n)| *n)
                        {
                            println!("\t{:>12}: {:>10} collisions", name, ncollisions)
                        }
                    }
                }
                TestResult::discard()
            }
        }
        fn hash_once<H: Hasher + Default>(t: impl Hash) -> u64 {
            let mut hasher = H::default();
            t.hash(&mut hasher);
            hasher.finish()
        }
    }
}
