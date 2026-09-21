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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::SignedMessage;
    use quickcheck_macros::quickcheck;

    #[quickcheck]
    fn matches_buffered_encoding(msg: SignedMessage) -> bool {
        let bytes = fvm_ipld_encoding::to_vec(&msg).unwrap();
        Cid::from_cbor_blake2b256(&msg).unwrap()
            == Cid::from_cbor_encoded_raw_bytes_blake2b256(&bytes)
    }
}
