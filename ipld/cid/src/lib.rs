// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

mod to_cid;

pub use self::to_cid::ToCid;
pub use dep_cid::{Cid as BaseCid, Codec, Error, Prefix, Version};
use std::ops::{Deref, DerefMut};

/// Representation of an IPLD Cid
#[derive(PartialEq, Eq, Clone, Debug, Hash)]
pub struct Cid {
    cid: BaseCid,
}

impl From<BaseCid> for Cid {
    fn from(cid: BaseCid) -> Self {
        Self { cid }
    }
}

impl Default for Cid {
    fn default() -> Self {
        Self {
            cid: BaseCid::new(Codec::Raw, Version::V0, &[]),
        }
    }
}

impl Deref for Cid {
    type Target = BaseCid;
    fn deref(&self) -> &Self::Target {
        &self.cid
    }
}

impl DerefMut for Cid {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.cid
    }
}

impl Cid {
    /// Cid constructor
    pub fn new(cid: BaseCid) -> Self {
        Self { cid }
    }
    /// Constructs a v0 cid with a given codec and bytes
    pub fn from_bytes_v0<B>(codec: Codec, bz: B) -> Self
    where
        B: AsRef<[u8]>,
    {
        Self {
            cid: BaseCid::new(codec, Version::V0, bz.as_ref()),
        }
    }
    /// Constructs a v1 cid with a given codec and bytes
    pub fn from_bytes_v1<B>(codec: Codec, bz: B) -> Self
    where
        B: AsRef<[u8]>,
    {
        Self {
            cid: BaseCid::new(codec, Version::V1, bz.as_ref()),
        }
    }

    /// Create a new CID from raw data (binary or multibase encoded string)
    pub fn from<T: ToCid>(data: T) -> Result<Cid, Error> {
        data.to_cid()
    }

    /// Create a new CID from a prefix and some data.
    pub fn new_from_prefix(prefix: &Prefix, data: &[u8]) -> Cid {
        let mut hash = multihash::encode(prefix.mh_type.to_owned(), data).unwrap();
        hash.truncate(prefix.mh_len + 2);
        BaseCid {
            version: prefix.version,
            codec: prefix.codec.to_owned(),
            hash,
        }
        .into()
    }
}
