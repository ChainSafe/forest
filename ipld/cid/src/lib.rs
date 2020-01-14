// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

mod to_cid;

pub use self::to_cid::ToCid;
pub use dep_cid::{Cid as BaseCid, Codec, Error, Prefix, Version};
use encoding::{de, ser, serde_bytes, tags::Tagged};
use std::ops::{Deref, DerefMut};

const CBOR_TAG_CID: u64 = 42;

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

impl ser::Serialize for Cid {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let cid_bytes = self.cid.to_bytes();
        let value = serde_bytes::Bytes::new(&cid_bytes);
        Tagged::new(Some(CBOR_TAG_CID), &value).serialize(s)
    }
}

impl<'de> de::Deserialize<'de> for Cid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let tagged = Tagged::<serde_bytes::ByteBuf>::deserialize(deserializer)?;
        match tagged.tag {
            // TODO verify this
            Some(CBOR_TAG_CID) | None => Ok(tagged
                .value
                .to_vec()
                .to_cid()
                .map_err(|e| de::Error::custom(e.to_string()))?),
            Some(_) => Err(de::Error::custom("unexpected tag")),
        }
    }
}

impl Cid {
    /// Cid constructor
    pub fn new(cid: BaseCid) -> Self {
        Self { cid }
    }

    /// Constructs a cid with bytes using default version and codec
    pub fn from_bytes_default<B: AsRef<[u8]>>(bz: B) -> Self {
        Self {
            cid: BaseCid::new(Codec::DagCBOR, Version::V1, bz.as_ref()),
        }
    }

    /// Create a new CID from raw data (binary or multibase encoded string)
    pub fn from_raw<T: ToCid>(data: T) -> Result<Cid, Error> {
        data.to_cid()
    }

    /// Create a new CID from a prefix and some data.
    pub fn new_from_prefix(prefix: &Prefix, data: &[u8]) -> Cid {
        let mut hash = multihash::encode(prefix.mh_type.to_owned(), data).unwrap();
        hash.truncate(prefix.mh_len + 2);
        Cid::from(BaseCid {
            version: prefix.version,
            codec: prefix.codec.to_owned(),
            hash,
        })
    }
}
