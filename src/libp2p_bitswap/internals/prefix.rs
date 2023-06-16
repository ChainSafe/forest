// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::convert::TryFrom;

use libipld::cid::{
    multihash::{Code, MultihashDigest},
    Version,
};
use unsigned_varint::{decode as varint_decode, encode as varint_encode};

use crate::libp2p_bitswap::*;

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

impl Prefix {
    /// Create a new prefix from encoded bytes.
    pub fn new(data: &[u8]) -> anyhow::Result<Prefix> {
        let (raw_version, remain) = varint_decode::u64(data)?;
        let version = Version::try_from(raw_version)?;
        let (codec, remain) = varint_decode::u64(remain)?;
        let (mh_type, remain) = varint_decode::u64(remain)?;
        let (mh_len, _remain) = varint_decode::usize(remain)?;
        Ok(Prefix {
            version,
            codec,
            mh_type,
            mh_len,
        })
    }

    /// Convert the prefix to encoded bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut res = Vec::with_capacity(4);
        let mut buf = varint_encode::u64_buffer();
        let version = varint_encode::u64(self.version.into(), &mut buf);
        res.extend_from_slice(version);
        let mut buf = varint_encode::u64_buffer();
        let codec = varint_encode::u64(self.codec, &mut buf);
        res.extend_from_slice(codec);
        let mut buf = varint_encode::u64_buffer();
        let mh_type = varint_encode::u64(self.mh_type, &mut buf);
        res.extend_from_slice(mh_type);
        let mut buf = varint_encode::u64_buffer();
        let mh_len = varint_encode::u64(self.mh_len as u64, &mut buf);
        res.extend_from_slice(mh_len);
        res
    }

    /// Create a CID out of the prefix and some data that will be hashed
    pub fn to_cid(&self, data: &[u8]) -> anyhow::Result<Cid> {
        let mh = Code::try_from(self.mh_type)?.digest(data);
        Ok(Cid::new(self.version, self.codec, mh)?)
    }
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
