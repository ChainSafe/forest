// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::{
    multihash::{Code::Blake2b256, MultihashDigest},
    Cid,
};

/// Extension methods for constructing `dag-cbor` [Cid]
pub trait CidCborExt {
    /// Default CID builder for Filecoin
    ///
    /// - The default codec is CBOR
    /// - The default hash function is 256 bit BLAKE2b
    ///
    /// This matches [`abi.CidBuilder`](https://github.com/filecoin-project/go-state-types/blob/master/abi/cid.go#L49) in go
    fn new_dag_cbor_default<S: serde::ser::Serialize>(obj: &S) -> anyhow::Result<Cid> {
        let bytes = fvm_ipld_encoding3::to_vec(obj)?;
        Ok(Cid::new_v1(
            fvm_ipld_encoding3::DAG_CBOR,
            Blake2b256.digest(&bytes),
        ))
    }
}

impl CidCborExt for Cid {}
