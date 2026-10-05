// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::utils::multihash::prelude::*;
use cid::Cid;
use fvm_ipld_encoding::Error;
use multihash_derive::Hasher as _;

/// Extension methods for constructing `dag-cbor` [Cid]
pub trait CidCborExt {
    /// Default CID builder for Filecoin
    ///
    /// - The default codec is [`fvm_ipld_encoding::DAG_CBOR`]
    /// - The default hash function is 256 bit BLAKE2b
    ///
    /// This matches [`abi.CidBuilder`](https://github.com/filecoin-project/go-state-types/blob/master/abi/cid.go#L49) in go
    fn from_cbor_blake2b256<S: serde::ser::Serialize>(obj: &S) -> Result<Cid, Error> {
        let mut hasher = multihash_codetable::Blake2b256::default();
        fvm_ipld_encoding::to_writer(&mut hasher, obj)?;
        let digest = MultihashCode::Blake2b256
            .wrap(hasher.finalize())
            .expect("BLAKE2b-256 digest is 32 bytes, within the multihash allocation");
        Ok(Cid::new_v1(fvm_ipld_encoding::DAG_CBOR, digest))
    }

    /// Build CID v1 with `Blake2b256` hasher from `DAG_CBOR` encoded bytes
    fn from_cbor_encoded_raw_bytes_blake2b256(bytes: &[u8]) -> Cid {
        Cid::new_v1(
            fvm_ipld_encoding::DAG_CBOR,
            MultihashCode::Blake2b256.digest(bytes),
        )
    }
}

impl CidCborExt for Cid {}

/// The BLAKE2b-256 digest a value's `DAG_CBOR` [`Cid`] is built from, and the length of that
/// encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct EncodedCbor {
    digest: [u8; 32],
    len: usize,
}

impl EncodedCbor {
    /// Encodes `obj` once, keeping only what callers need afterwards.
    pub(crate) fn compute<S: serde::ser::Serialize>(obj: &S) -> Result<Self, Error> {
        let mut writer = CountingHasher::default();
        fvm_ipld_encoding::to_writer(&mut writer, obj)?;
        Ok(Self {
            digest: writer
                .hasher
                .finalize()
                .try_into()
                .expect("BLAKE2b-256 produces a 32 byte digest"),
            len: writer.written,
        })
    }

    pub(crate) fn cid(&self) -> Cid {
        let digest = MultihashCode::Blake2b256
            .wrap(&self.digest)
            .expect("BLAKE2b-256 digest is 32 bytes, within the multihash allocation");
        Cid::new_v1(fvm_ipld_encoding::DAG_CBOR, digest)
    }

    pub(crate) fn byte_len(&self) -> usize {
        self.len
    }
}

/// A lazily computed [`EncodedCbor`].
///
/// Equality, hashing and [`Debug`] ignore it, so a type holding one derives those traits as if the
/// field were absent. Without that, two values with identical fields would compare unequal once
/// either computed its memo.
#[derive(Clone, Default)]
pub(crate) struct Memo(std::sync::OnceLock<EncodedCbor>);

impl Memo {
    pub(crate) fn get_or_init(&self, compute: impl FnOnce() -> EncodedCbor) -> &EncodedCbor {
        self.0.get_or_init(compute)
    }

    pub(crate) fn clear(&mut self) {
        self.0.take();
    }
}

impl std::fmt::Debug for Memo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Memo").finish_non_exhaustive()
    }
}

impl PartialEq for Memo {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for Memo {}

impl std::hash::Hash for Memo {
    fn hash<H: std::hash::Hasher>(&self, _: &mut H) {}
}

impl get_size2::GetSize for Memo {}

/// Feeds the hasher while counting, so one pass yields both halves of an [`EncodedCbor`].
#[derive(Default)]
struct CountingHasher {
    hasher: multihash_codetable::Blake2b256,
    written: usize,
}

impl std::io::Write for CountingHasher {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.hasher.update(buf);
        self.written += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::SignedMessage;
    use quickcheck_macros::quickcheck;

    fn warm(payload: &[u8]) -> Memo {
        let memo = Memo::default();
        memo.get_or_init(|| EncodedCbor::compute(&payload).unwrap());
        memo
    }

    #[quickcheck]
    fn cid_matches_the_unmemoized_builder(payload: Vec<u8>) -> bool {
        EncodedCbor::compute(&payload).unwrap().cid()
            == Cid::from_cbor_blake2b256(&payload).unwrap()
    }

    #[quickcheck]
    fn byte_len_counts_every_encoded_byte(payload: Vec<u8>) -> bool {
        EncodedCbor::compute(&payload).unwrap().byte_len()
            == fvm_ipld_encoding::to_vec(&payload).unwrap().len()
    }

    #[quickcheck]
    fn clear_drops_a_computed_value(payload: Vec<u8>, replacement: Vec<u8>) -> bool {
        let mut memo = warm(&payload);
        memo.clear();
        let after = *memo.get_or_init(|| EncodedCbor::compute(&replacement).unwrap());
        after == EncodedCbor::compute(&replacement).unwrap()
    }

    /// `to_writer` may hand the hasher one chunk or many, and never calls `flush`.
    #[quickcheck]
    fn counting_hasher_accumulates_across_writes(chunks: Vec<Vec<u8>>) -> bool {
        use std::io::Write as _;

        let mut split = CountingHasher::default();
        for chunk in &chunks {
            split.write_all(chunk).unwrap();
        }
        split.flush().unwrap();

        let joined_bytes = chunks.concat();
        let mut joined = CountingHasher::default();
        joined.write_all(&joined_bytes).unwrap();

        split.written == joined_bytes.len() && split.hasher.finalize() == joined.hasher.finalize()
    }

    #[test]
    fn get_or_init_computes_at_most_once() {
        let calls = std::cell::Cell::new(0);
        let memo = Memo::default();
        let compute = || {
            calls.set(calls.get() + 1);
            EncodedCbor::compute(&[0u8; 4]).unwrap()
        };
        assert_eq!(*memo.get_or_init(compute), *memo.get_or_init(compute));
        assert_eq!(calls.get(), 1);
    }

    #[quickcheck]
    fn matches_buffered_encoding(msg: SignedMessage) -> bool {
        let bytes = fvm_ipld_encoding::to_vec(&msg).unwrap();
        Cid::from_cbor_blake2b256(&msg).unwrap()
            == Cid::from_cbor_encoded_raw_bytes_blake2b256(&bytes)
    }
}
