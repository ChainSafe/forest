// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod codec;
mod error;
mod to_cid;
mod version;

pub use self::codec::Codec;
pub use self::error::Error;
pub use self::version::Version;
use integer_encoding::{VarIntReader, VarIntWriter};
pub use multihash;
use multihash::{Hash, Multihash};
use std::convert::TryInto;
use std::fmt;
use std::io::Cursor;

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
pub struct Prefix {
    pub version: Version,
    pub codec: Codec,
    pub mh_type: Hash,
    pub mh_len: usize,
}

/// Representation of a IPLD CID.
#[derive(Eq, Clone, Debug)]
pub struct Cid {
    pub version: Version,
    pub codec: Codec,
    pub hash: Multihash,
}

impl Default for Cid {
    fn default() -> Self {
        Self::new(
            Codec::Raw,
            Version::V1,
            multihash::encode(Hash::Blake2b256, &[]).unwrap(),
        )
    }
}

#[cfg(feature = "serde")]
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

#[cfg(feature = "serde")]
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
    pub fn from_bytes(bz: &[u8], hash: Hash) -> Result<Self, Error> {
        let prefix = Prefix {
            version: Version::V1,
            codec: Codec::DagCBOR,
            mh_type: hash,
            mh_len: (hash.size()) as usize,
        };
        Ok(Self::new_from_prefix(&prefix, bz)?)
    }

    /// Create a new CID from raw data (binary or multibase encoded string)
    pub fn from_raw_cid<T: TryInto<Cid>>(data: T) -> Result<Cid, T::Error> {
        data.try_into()
    }

    /// Create a new CID from a prefix and some data.
    pub fn new_from_prefix(prefix: &Prefix, data: &[u8]) -> Result<Cid, Error> {
        let hash = multihash::encode(prefix.mh_type.to_owned(), data)?;
        Ok(Cid {
            version: prefix.version,
            codec: prefix.codec.to_owned(),
            hash,
        })
    }

    fn to_string_v0(&self) -> String {
        use multibase::{encode, Base};

        let mut string = encode(Base::Base58btc, self.hash.clone());

        // Drop the first character as v0 does not know
        // about multibase
        string.remove(0);

        string
    }

    fn to_string_v1(&self) -> String {
        use multibase::{encode, Base};

        encode(Base::Base58btc, self.to_bytes().as_slice())
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

    /// Returns prefix for Cid format
    pub fn prefix(&self) -> Prefix {
        Prefix {
            version: self.version,
            codec: self.codec.to_owned(),
            mh_type: self.hash.algorithm(),
            mh_len: self.hash.as_bytes().len(),
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

impl Prefix {
    /// Generate new prefix from encoded bytes
    pub fn new_from_bytes(data: &[u8]) -> Result<Prefix, Error> {
        let mut cur = Cursor::new(data);

        let raw_version = cur.read_varint()?;
        let raw_codec = cur.read_varint()?;
        let raw_mh_type: u64 = cur.read_varint()?;

        let version = Version::from(raw_version)?;
        let codec = Codec::from(raw_codec)?;

        let mh_type = Hash::from_code(raw_mh_type as u16).ok_or(Error::ParsingError)?;

        let mh_len = cur.read_varint()?;

        Ok(Prefix {
            version,
            codec,
            mh_type,
            mh_len,
        })
    }

    /// Encodes prefix to bytes
    pub fn as_bytes(&self) -> Vec<u8> {
        let mut res = Vec::with_capacity(4);

        // io can't fail on Vec
        res.write_varint(u64::from(self.version)).unwrap();
        res.write_varint(u64::from(self.codec)).unwrap();
        res.write_varint(self.mh_type.code() as u64).unwrap();
        res.write_varint(self.mh_len as u64).unwrap();

        res
    }
}
