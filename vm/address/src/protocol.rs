// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{from_leb_bytes, to_leb_bytes, Error, BLS_PUB_LEN, PAYLOAD_HASH_LEN};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use std::convert::TryInto;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::u64;

#[derive(Copy, Clone)]
pub struct BLSPublicKey(pub [u8; BLS_PUB_LEN]);

impl Hash for BLSPublicKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(&self.0);
    }
}

impl Eq for BLSPublicKey {}
impl PartialEq for BLSPublicKey {
    fn eq(&self, other: &Self) -> bool {
        self.0[..].eq(&other.0[..])
    }
}

impl fmt::Debug for BLSPublicKey {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        self.0[..].fmt(formatter)
    }
}

impl From<[u8; BLS_PUB_LEN]> for BLSPublicKey {
    fn from(pk: [u8; BLS_PUB_LEN]) -> Self {
        BLSPublicKey(pk)
    }
}

impl Deref for BLSPublicKey {
    type Target = [u8; BLS_PUB_LEN];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Protocol defines the addressing protocol used to derive data to an address
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub enum Payload {
    /// ID protocol addressing, encoded as leb128 bytes.
    ID(u64),
    /// SECP256K1 key addressing
    Secp256k1([u8; PAYLOAD_HASH_LEN]),
    /// Actor protocol addressing
    Actor([u8; PAYLOAD_HASH_LEN]),
    /// BLS key addressing
    BLS(BLSPublicKey),
}

impl Payload {
    /// Returns encoded bytes of Address
    pub fn to_bytes(&self) -> Vec<u8> {
        use Payload::*;
        match self {
            ID(i) => to_leb_bytes(*i).unwrap(),
            Secp256k1(arr) => arr.to_vec(),
            Actor(arr) => arr.to_vec(),
            BLS(arr) => arr.to_vec(),
        }
    }

    /// Generates payload from encoded bytes
    pub fn new(protocol: Protocol, payload: &[u8]) -> Result<Self, Error> {
        let payload = match protocol {
            Protocol::ID => Self::ID(from_leb_bytes(payload)?),
            Protocol::Secp256k1 => Self::Secp256k1(
                payload
                    .try_into()
                    .map_err(|_| Error::InvalidPayloadLength(payload.len()))?,
            ),
            Protocol::Actor => Self::Actor(
                payload
                    .try_into()
                    .map_err(|_| Error::InvalidPayloadLength(payload.len()))?,
            ),
            Protocol::BLS => {
                if payload.len() != BLS_PUB_LEN {
                    return Err(Error::InvalidBLSLength(payload.len()));
                }
                let mut pk = [0u8; BLS_PUB_LEN];
                pk.copy_from_slice(payload);
                Self::BLS(pk.into())
            }
        };
        Ok(payload)
    }
}

impl From<Payload> for Protocol {
    fn from(pl: Payload) -> Self {
        match pl {
            Payload::ID(_) => Self::ID,
            Payload::Secp256k1(_) => Self::Secp256k1,
            Payload::Actor(_) => Self::Actor,
            Payload::BLS(_) => Self::BLS,
        }
    }
}

impl Default for Payload {
    fn default() -> Self {
        Payload::ID(u64::MAX)
    }
}

/// Protocol defines the addressing protocol used to derive data to an address
#[derive(PartialEq, Eq, Copy, Clone, FromPrimitive, Debug, Hash)]
pub enum Protocol {
    /// ID protocol addressing
    ID = 0,
    /// SECP256K1 key addressing
    Secp256k1 = 1,
    /// Actor protocol addressing
    Actor = 2,
    /// BLS key addressing
    BLS = 3,
}

impl Default for Protocol {
    fn default() -> Self {
        Protocol::ID
    }
}

impl Protocol {
    /// from_byte allows referencing back to Protocol from encoded byte
    pub(super) fn from_byte(b: u8) -> Option<Protocol> {
        FromPrimitive::from_u8(b)
    }
}

/// allows conversion of Protocol value to string
impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let i = *self as u8;
        write!(f, "{}", i)
    }
}
