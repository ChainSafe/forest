// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::{Cid, Codec, Error, Version};
use integer_encoding::VarIntReader;
use multibase;
use multihash::Multihash;
use std::io::Cursor;
use std::str::FromStr;

/// Trait used to convert objects to Cid (Currently not necessary, but keeping in line with dep)
pub trait ToCid {
    fn to_cid(&self) -> Result<Cid, Error>;
}

impl ToCid for Vec<u8> {
    /// Create a Cid from a byte vector.
    #[inline]
    fn to_cid(&self) -> Result<Cid, Error> {
        self.as_slice().to_cid()
    }
}

impl ToCid for String {
    /// Create a Cid from an owned String.
    #[inline]
    fn to_cid(&self) -> Result<Cid, Error> {
        self.as_str().to_cid()
    }
}

impl<'a> ToCid for &'a str {
    #[inline]
    fn to_cid(&self) -> Result<Cid, Error> {
        ToCid::to_cid(*self)
    }
}

impl ToCid for str {
    fn to_cid(&self) -> Result<Cid, Error> {
        static IPFS_DELIMETER: &str = "/ipfs/";

        let hash = match self.find(IPFS_DELIMETER) {
            Some(index) => &self[index + IPFS_DELIMETER.len()..],
            _ => self,
        };

        if hash.len() < 2 {
            return Err(Error::InputTooShort);
        }

        let (_, decoded) = if Version::is_v0_str(hash) {
            // TODO: could avoid the roundtrip here and just use underlying
            // base-x base58btc decoder here.
            let hash = multibase::Base::Base58btc.code().to_string() + hash;

            multibase::decode(hash)
        } else {
            multibase::decode(hash)
        }?;

        decoded.to_cid()
    }
}

impl FromStr for Cid {
    type Err = Error;
    fn from_str(src: &str) -> Result<Self, Error> {
        src.to_cid()
    }
}

impl<'a> ToCid for &'a [u8] {
    #[inline]
    fn to_cid(&self) -> Result<Cid, Error> {
        ToCid::to_cid(*self)
    }
}

impl ToCid for [u8] {
    /// Create a Cid from a byte slice.
    fn to_cid(&self) -> Result<Cid, Error> {
        if Version::is_v0_binary(self) {
            // Verify that hash can be decoded, this is very cheap
            let hash = Multihash::from_bytes(self.to_vec())?;

            Ok(Cid::new(Codec::DagCBOR, Version::V0, hash))
        } else {
            let mut cur = Cursor::new(self);
            let raw_version = cur.read_varint()?;
            let raw_codec = cur.read_varint()?;

            let version = Version::from(raw_version)?;
            let codec = Codec::from(raw_codec)?;

            let hash = &self[cur.position() as usize..];

            // Verify that hash can be decoded, this is very cheap
            // TODO verify this (was previously using all bytes)
            let hash = Multihash::from_bytes(hash.to_vec())?;

            Ok(Cid::new(codec, version, hash))
        }
    }
}
