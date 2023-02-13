// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use cid::Cid;
use libipld::cid::Version;

/// Prefix represents all metadata of a CID, without the actual content.
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Prefix {
    /// The version of `CID`.
    pub version: Version,
    /// The codec of `CID`.
    pub codec: u64,
    /// The `multihash` type of `CID`.
    pub mh_type: u64,
    /// The `multihash` length of `CID`.
    pub mh_len: usize,
}

impl From<&Cid> for Prefix {
    fn from(cid: &Cid) -> Self {
        Self {
            version: cid.version(),
            codec: cid.codec(),
            mh_type: cid.hash().code(),
            mh_len: cid.hash().digest().len(),
        }
    }
}
