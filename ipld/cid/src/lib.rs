// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod codec;
mod error;
mod mh_code;
mod prefix;
mod to_cid;
mod version;

pub use self::codec::Codec;
pub use self::error::Error;
pub use self::mh_code::*;
pub use self::prefix::Prefix;
pub use self::version::Version;
use integer_encoding::VarIntWriter;
pub use multihash;
use multihash::derive::Multihash;
use multihash::typenum::U32;
use multihash::MultihashDigest;
use std::convert::{TryFrom, TryInto};
use std::fmt;

#[cfg(feature = "cbor")]
use serde::{de, ser};
#[cfg(feature = "cbor")]
use serde_cbor::tags::Tagged;

#[cfg(feature = "cbor")]
const CBOR_TAG_CID: u64 = 42;
/// multibase identity prefix
/// https://github.com/ipld/specs/blob/master/block-layer/codecs/dag-cbor.md#link-format
#[cfg(feature = "cbor")]
const MULTIBASE_IDENTITY: u8 = 0;

#[cfg(feature = "json")]
pub mod json;

/// Representation of a IPLD CID.
#[derive(PartialEq, Eq, Clone, Copy)]
pub struct Cid {
    pub version: Version,
    pub codec: Codec,
    pub hash: Multihash,
}

#[allow(clippy::derive_hash_xor_eq)]
impl std::hash::Hash for Cid {
    fn hash<T: std::hash::Hasher>(&self, state: &mut T) {
        // * Manual implementation is required because of generic size on multihash
        self.version.hash(state);
        self.codec.hash(state);
        self.hash.code().hash(state);
        self.hash.digest().hash(state);
    }
}

impl Default for Cid {
    fn default() -> Self {
        Self::new(Codec::Raw, Version::V0, Code::Identity.digest(&[]))
    }
}

#[cfg(feature = "cbor")]
impl ser::Serialize for Cid {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        if self == &Cid::default() {
            // TODO remove if intended to use outside of Forest
            // Only used for convenience of having Cid implement default
            return Err(ser::Error::custom("Cannot serialize a default Cid"));
        }

        let mut cid_bytes = self.to_bytes();

        // or for all Cid bytes (byte is irrelevant and redundant)
        cid_bytes.insert(0, MULTIBASE_IDENTITY);

        let value = serde_bytes::Bytes::new(&cid_bytes);
        Tagged::new(Some(CBOR_TAG_CID), &value).serialize(s)
    }
}

#[cfg(feature = "cbor")]
impl<'de> de::Deserialize<'de> for Cid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let tagged = Tagged::<serde_bytes::ByteBuf>::deserialize(deserializer)?;
        match tagged.tag {
            Some(CBOR_TAG_CID) | None => {
                let mut bz = tagged.value.into_vec();

                if bz.first() == Some(&MULTIBASE_IDENTITY) {
                    bz.remove(0);
                }

                Ok(Cid::try_from(bz)
                    .map_err(|e| de::Error::custom(format!("Failed to deserialize Cid: {}", e)))?)
            }
            Some(_) => Err(de::Error::custom("unexpected tag")),
        }
    }
}

impl Cid {
    /// Create a new CID.
    pub fn new(codec: Codec, version: Version, hash: Multihash) -> Cid {
        Cid {
            version,
            codec,
            hash,
        }
    }

    /// Create a new v1 CID.
    pub fn new_v1(codec: Codec, hash: Multihash) -> Cid {
        Cid::new(codec, Version::V1, hash)
    }

    /// Create a new v0 CID.
    pub fn new_v0(codec: Codec, hash: Multihash) -> Cid {
        Cid::new(codec, Version::V0, hash)
    }

    /// Constructs a cid with bytes using default version and codec
    pub fn new_from_cbor<T: MultihashDigest<AllocSize = U32>>(bz: &[u8], hash: T) -> Self {
        let hash = hash.digest(bz);
        Cid {
            version: Version::V1,
            codec: Codec::DagCBOR,
            hash,
        }
    }

    /// Create a new CID from raw data (binary or multibase encoded string)
    pub fn from_raw_cid<T: TryInto<Cid>>(data: T) -> Result<Cid, T::Error> {
        data.try_into()
    }

    /// Create a new CID from a prefix and some data.
    pub fn new_from_prefix(prefix: &Prefix, data: &[u8]) -> Result<Cid, Error> {
        let hash: Multihash = Code::try_from(prefix.mh_type)
            .map_err(|e| Error::Other(e.to_string()))?
            .digest(data);
        Ok(Cid {
            version: prefix.version,
            codec: prefix.codec.to_owned(),
            hash,
        })
    }

    fn to_string_v0(&self) -> String {
        use multibase::{encode, Base};

        let mut string = encode(Base::Base58Btc, self.hash.to_bytes());

        // Drop the first character as v0 does not know
        // about multibase
        string.remove(0);

        string
    }

    fn to_string_v1(&self) -> String {
        use multibase::{encode, Base};

        encode(Base::Base32Lower, self.to_bytes().as_slice())
    }

    fn to_bytes_v0(&self) -> Vec<u8> {
        self.hash.to_bytes()
    }

    fn to_bytes_v1(&self) -> Vec<u8> {
        let mut res = Vec::with_capacity(32);
        res.write_varint(u64::from(self.version)).unwrap();
        res.write_varint(u64::from(self.codec)).unwrap();
        // TODO avoid allocation here
        res.append(&mut self.hash.to_bytes());

        res
    }

    /// Returns encoded bytes of a cid
    pub fn to_bytes(&self) -> Vec<u8> {
        match self.version {
            Version::V0 => self.to_bytes_v0(),
            Version::V1 => self.to_bytes_v1(),
        }
    }

    /// Returns prefix for Cid format
    pub fn prefix(&self) -> Prefix {
        Prefix {
            version: self.version,
            codec: self.codec.to_owned(),
            mh_type: self.hash.code(),
            mh_len: self.hash.size() as usize,
        }
    }

    /// Returns cid in bytes to be stored in datastore
    pub fn key(&self) -> Vec<u8> {
        self.to_bytes()
    }
}

impl fmt::Display for Cid {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let encoded = match self.version {
            Version::V0 => self.to_string_v0(),
            Version::V1 => self.to_string_v1(),
        };
        write!(f, "{}", encoded)
    }
}

impl fmt::Debug for Cid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cid(\"{}\")", self)
    }
}
