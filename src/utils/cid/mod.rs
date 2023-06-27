// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{
    multihash::{Code, MultihashDigest},
    Cid, Version,
};
use fvm_ipld_encoding::DAG_CBOR;

/// Extension methods for constructing `dag-cbor` [Cid]
pub trait CidCborExt {
    /// Default CID builder for Filecoin
    ///
    /// - The default codec is [`fvm_ipld_encoding3::DAG_CBOR`]
    /// - The default hash function is 256 bit BLAKE2b
    ///
    /// This matches [`abi.CidBuilder`](https://github.com/filecoin-project/go-state-types/blob/master/abi/cid.go#L49) in go
    fn from_cbor_blake2b256<S: serde::ser::Serialize>(obj: &S) -> anyhow::Result<Cid> {
        let bytes = fvm_ipld_encoding3::to_vec(obj)?;
        Ok(Cid::new_v1(
            fvm_ipld_encoding3::DAG_CBOR,
            Code::Blake2b256.digest(&bytes),
        ))
    }
}

impl CidCborExt for Cid {}

pub const BLAKE2B256_SIZE: usize = 32;

// `CidVariant` is an enumeration of known CID types that are used in the Filecoin blockchain. CIDs
// contain a significant amount of static data (such as version, codec, hash identifier, hash
// length). This static data represented by a single tag in the enum.
//
// Nearly all Filecoin CIDs are V1, DagCbor encoded, and hashed with Blake2b256 (which has a hash
// length of 256bits). Naively representing such a CID requires 96 bytes but `CidVariant` does it in
// only 40 bytes. If other types of CID become popular, they can be added to the CidVariant
// structure.
pub enum CidVariant {
    V1DagCborBlake2b([u8; BLAKE2B256_SIZE]),
}

impl TryFrom<Cid> for CidVariant {
    type Error = ();
    fn try_from(cid: Cid) -> Result<Self, Self::Error> {
        if cid.version() == Version::V1 && cid.codec() == DAG_CBOR {
            if let Ok(small_hash) = cid.hash().resize() {
                let (code, bytes, size) = small_hash.into_inner();
                if code == u64::from(Code::Blake2b256) && size as usize == BLAKE2B256_SIZE {
                    return Ok(CidVariant::V1DagCborBlake2b(bytes));
                }
            }
        }
        Err(())
    }
}

#[cfg(test)]
mod tests {
    use super::CidVariant;
    use super::*;
    use crate::db::MemoryDB;
    use crate::utils::db::CborStoreExt;
    use anyhow::*;
    use cid::{
        multihash::{Code, MultihashDigest},
        Cid,
    };
    use fvm_ipld_encoding::DAG_CBOR;
    use quickcheck_macros::quickcheck;
    use std::mem::size_of;

    #[quickcheck]
    fn test_cid_cbor_ext(s: String) -> Result<()> {
        let cid1 = Cid::from_cbor_blake2b256(&s)?;
        let cid2 = {
            let store = MemoryDB::default();
            store.put_cbor_default(&s)?
        };
        ensure!(cid1 == cid2);

        Ok(())
    }

    // If this stops being true, please update the documentation above.
    #[test]
    fn cid_size_assumption() {
        assert_eq!(size_of::<Cid>(), 96);
    }

    // If this stops being true, please update the BLAKE2B256_SIZE constant.
    #[test]
    fn blake_size_assumption() {
        assert_eq!(
            Code::Blake2b256.digest(&[]).size() as usize,
            super::BLAKE2B256_SIZE
        );
    }

    #[test]
    fn known_v1_blake2b() {
        let cid = Cid::new(
            cid::Version::V1,
            DAG_CBOR,
            Code::Blake2b256.digest("blake".as_bytes()),
        )
        .unwrap();
        assert!(matches!(
            cid.try_into().unwrap(),
            CidVariant::V1DagCborBlake2b(_)
        ));
    }

    // If this test fails, the default encoding is no longer v1+dagcbor+blake2b. Add the new default
    // CID type to `CidVariant`.
    #[test]
    fn default_is_v1_dagcbor() {
        let cid = MemoryDB::default().put_cbor_default(&()).unwrap();
        assert!(matches!(
            cid.try_into().unwrap(),
            CidVariant::V1DagCborBlake2b(_)
        ));
    }
}
