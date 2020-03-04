// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod codec;
mod error;
mod to_cid;
mod version;

pub use self::codec::Codec;
pub use self::error::Error;
pub use self::version::Version;
use integer_encoding::VarIntWriter;
pub use multihash;
use multihash::{Blake2b256, Multihash, MultihashDigest};
use std::convert::TryInto;
use std::fmt;

#[cfg(feature = "serde_derive")]
use serde::{de, ser};
#[cfg(feature = "serde_derive")]
use serde_cbor::tags::Tagged;
#[cfg(feature = "serde_derive")]
use std::convert::TryFrom;

#[cfg(feature = "serde_derive")]
const CBOR_TAG_CID: u64 = 42;
/// multibase identity prefix
/// https://github.com/ipld/specs/blob/master/block-layer/codecs/dag-cbor.md#link-format
#[cfg(feature = "serde_derive")]
const MULTIBASE_IDENTITY: u8 = 0;

/// Prefix represents all metadata of a CID, without the actual content.
#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Prefix<T: MultihashDigest> {
    pub version: Version,
    pub codec: Codec,
    pub mh_type: T,
}

/// Representation of a IPLD CID.
#[derive(Eq, Clone)]
pub struct Cid {
    pub version: Version,
    pub codec: Codec,
    pub hash: Multihash,
}

impl Default for Cid {
    fn default() -> Self {
        Self::new(Codec::Raw, Version::V1, Blake2b256.digest(&[]))
    }
}

#[cfg(feature = "serde_derive")]
impl ser::Serialize for Cid {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        let mut cid_bytes = self.to_bytes();

        // TODO determine if identity multibase prefix should just be included for IPLD links
        // or for all Cid bytes (byte is irrelevant and redundant)
        cid_bytes.insert(0, MULTIBASE_IDENTITY);

        let value = serde_bytes::Bytes::new(&cid_bytes);
        Tagged::new(Some(CBOR_TAG_CID), &value).serialize(s)
    }
}

#[cfg(feature = "serde_derive")]
impl<'de> de::Deserialize<'de> for Cid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let tagged = Tagged::<serde_bytes::ByteBuf>::deserialize(deserializer)?;
        match tagged.tag {
            Some(CBOR_TAG_CID) | None => {
                let mut bz = tagged.value.to_vec();

                if bz.first() == Some(&MULTIBASE_IDENTITY) {
                    bz.remove(0);
                }

                Ok(Cid::try_from(bz).map_err(|e| de::Error::custom(e.to_string()))?)
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

    /// Constructs a cid with bytes using default version and codec
    pub fn from_bytes<T: MultihashDigest>(bz: &[u8], hash: T) -> Result<Self, Error> {
        let prefix = Prefix {
            version: Version::V1,
            codec: Codec::DagCBOR,
            mh_type: hash,
        };
        Ok(Self::new_from_prefix(&prefix, bz)?)
    }

    /// Create a new CID from raw data (binary or multibase encoded string)
    pub fn from_raw_cid<T: TryInto<Cid>>(data: T) -> Result<Cid, T::Error> {
        data.try_into()
    }

    /// Create a new CID from a prefix and some data.
    pub fn new_from_prefix<T: MultihashDigest>(
        prefix: &Prefix<T>,
        data: &[u8],
    ) -> Result<Cid, Error> {
        let hash = prefix.mh_type.digest(data);
        Ok(Cid {
            version: prefix.version,
            codec: prefix.codec.to_owned(),
            hash,
        })
    }

    fn to_string_v0(&self) -> String {
        use multibase::{encode, Base};

        let mut string = encode(Base::Base58Btc, self.hash.clone());

        // Drop the first character as v0 does not know
        // about multibase
        string.remove(0);

        string
    }

    fn to_string_v1(&self) -> String {
        use multibase::{encode, Base};

        encode(Base::Base58Btc, self.to_bytes().as_slice())
    }

    fn to_bytes_v0(&self) -> Vec<u8> {
        self.hash.clone().into_bytes()
    }

    fn to_bytes_v1(&self) -> Vec<u8> {
        let mut res = Vec::with_capacity(16);
        res.write_varint(u64::from(self.version)).unwrap();
        res.write_varint(u64::from(self.codec)).unwrap();
        res.extend_from_slice(self.hash.as_bytes());

        res
    }

    /// Returns encoded bytes of a cid
    pub fn to_bytes(&self) -> Vec<u8> {
        match self.version {
            Version::V0 => self.to_bytes_v0(),
            Version::V1 => self.to_bytes_v1(),
        }
    }

    /// Returns cid in bytes to be stored in datastore
    pub fn key(&self) -> Vec<u8> {
        self.to_bytes()
    }
}

impl std::hash::Hash for Cid {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.to_bytes().hash(state);
    }
}

impl PartialEq for Cid {
    fn eq(&self, other: &Self) -> bool {
        self.to_bytes() == other.to_bytes()
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
