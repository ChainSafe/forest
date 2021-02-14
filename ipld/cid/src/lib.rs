// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

mod mh_code;
mod prefix;
mod to_cid;

pub use self::mh_code::{Code, Multihash, POSEIDON_BLS12_381_A1_FC1, SHA2_256_TRUNC254_PADDED};
pub use self::prefix::Prefix;
use cid::CidGeneric;
pub use cid::{Error, Version};
pub use multihash;
use multihash::MultihashDigest;
use std::convert::TryFrom;
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

/// Cbor [Cid] codec.
pub const DAG_CBOR: u64 = 0x71;
/// Sealed commitment [Cid] codec.
pub const FIL_COMMITMENT_SEALED: u64 = 0xf102;
/// Unsealed commitment [Cid] codec.
pub const FIL_COMMITMENT_UNSEALED: u64 = 0xf101;
/// Raw [Cid] codec. This represents data that is not encoded using any protocol.
pub const RAW: u64 = 0x55;

/// Constructs a cid with bytes using default version and codec
pub fn new_from_cbor(bz: &[u8], code: Code) -> Cid {
    let hash = code.digest(bz);
    Cid::new_v1(DAG_CBOR, hash)
}

/// Create a new CID from a prefix and some data.
pub fn new_from_prefix(prefix: &Prefix, data: &[u8]) -> Result<Cid, Error> {
    let hash: Multihash = Code::try_from(prefix.mh_type)?.digest(data);
    Cid::new(prefix.version, prefix.codec, hash)
}

/// Content identifier for any Ipld data. This Cid consists of a version, a codec (or serialization)
/// protocol and a multihash (hash of the Ipld data). Cids allow for hash linking, where the Cids
/// are used to resolve any arbitrary data over a network or from local storage.
#[derive(PartialEq, Eq, Clone, Copy, Default, Hash, PartialOrd, Ord)]
pub struct Cid(CidGeneric<multihash::U32>);

// This is just a wrapper around the rust-cid `Cid` type that is needed in order to make the
// interaction with Serde smoother.
impl Cid {
    /// Create a new CID.
    pub fn new(version: Version, codec: u64, hash: Multihash) -> Result<Self, Error> {
        Ok(Cid(CidGeneric::new(version, codec, hash)?))
    }

    /// Create a new CIDv1.
    pub fn new_v1(codec: u64, hash: Multihash) -> Self {
        Cid(CidGeneric::new_v1(codec, hash))
    }

    /// Returns the cid version.
    pub fn version(&self) -> Version {
        self.0.version()
    }

    /// Returns the cid codec.
    pub fn codec(&self) -> u64 {
        self.0.codec()
    }

    /// Returns the cid multihash.
    pub fn hash(&self) -> &Multihash {
        &self.0.hash()
    }

    /// Reads the bytes from a byte stream.
    pub fn read_bytes<R: std::io::Read>(reader: R) -> Result<Self, Error> {
        Ok(Cid(CidGeneric::read_bytes(reader)?))
    }

    /// Returns the encoded bytes of the `Cid`.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.to_bytes()
    }
}

#[cfg(feature = "cbor")]
impl ser::Serialize for Cid {
    fn serialize<S>(&self, s: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
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

impl fmt::Display for Cid {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for Cid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cid(\"{}\")", self)
    }
}
