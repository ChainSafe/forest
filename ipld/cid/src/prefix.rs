// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{Codec, Error, Version};
use integer_encoding::{VarIntReader, VarIntWriter};
use multihash::Code;
use std::io::Cursor;

/// Prefix represents all metadata of a CID, without the actual content.
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Prefix {
    pub version: Version,
    pub codec: Codec,
    pub mh_type: Code,
    pub mh_len: usize,
}

impl Prefix {
    /// Generate new prefix from encoded bytes
    pub fn new_from_bytes(data: &[u8]) -> Result<Prefix, Error> {
        let mut cur = Cursor::new(data);

        let raw_version = cur.read_varint()?;
        let raw_codec = cur.read_varint()?;
        let raw_mh_type: u64 = cur.read_varint()?;
        let mh_len: usize = cur.read_varint()?;

        let version = Version::from(raw_version)?;
        let codec = Codec::from(raw_codec)?;

        let mh_type = Code::from_u64(raw_mh_type);

        Ok(Prefix {
            version,
            codec,
            mh_type,
            mh_len,
        })
    }

    /// Encodes prefix to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut res = Vec::with_capacity(4);

        // io can't fail on Vec
        res.write_varint(u64::from(self.version)).unwrap();
        res.write_varint(u64::from(self.codec)).unwrap();
        res.write_varint(self.mh_type.to_u64()).unwrap();
        res.write_varint(self.mh_len).unwrap();

        res
    }
}
