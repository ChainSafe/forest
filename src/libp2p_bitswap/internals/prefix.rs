// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::libp2p_bitswap::*;
use crate::utils::multihash::prelude::*;
use cid::Version;
use std::convert::TryFrom;
use unsigned_varint::{decode as varint_decode, encode as varint_encode};

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
        let mh = MultihashCode::try_from(self.mh_type)?.checked_digest(data)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck_macros::quickcheck;

    /// For an identity prefix, `to_cid` succeeds exactly when the payload fits
    /// the identity buffer, and never panics on either side of the bound.
    #[quickcheck]
    fn prop_to_cid_total(data: Vec<u8>) -> bool {
        // The pad guarantees the second candidate exceeds the bound.
        let oversized = [data.as_slice(), &[0u8; 128]].concat();
        [data, oversized].into_iter().all(|candidate| {
            let prefix = Prefix {
                version: Version::V1,
                codec: 0x55,   // raw
                mh_type: 0x00, // identity
                mh_len: 0,     // unused by `to_cid`
            };
            prefix.to_cid(&candidate).is_ok()
                == (candidate.len() <= MultihashCode::IDENTITY_MAX_SIZE)
        })
    }
}
