// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{
    multihash::{Code, MultihashDigest},
    Cid,
};
use fvm_ipld_encoding::Error;

/// Extension methods for constructing `dag-cbor` [Cid]
pub trait CidCborExt {
    /// Default CID builder for Filecoin
    ///
    /// - The default codec is [`fvm_ipld_encoding::DAG_CBOR`]
    /// - The default hash function is 256 bit BLAKE2b
    ///
    /// This matches [`abi.CidBuilder`](https://github.com/filecoin-project/go-state-types/blob/master/abi/cid.go#L49) in go
    fn from_cbor_blake2b256<S: serde::ser::Serialize>(obj: &S) -> Result<Cid, Error> {
        let bytes = fvm_ipld_encoding::to_vec(obj)?;
        Ok(Cid::new_v1(
            fvm_ipld_encoding::DAG_CBOR,
            Code::Blake2b256.digest(&bytes),
        ))
    }
}

impl CidCborExt for Cid {}
