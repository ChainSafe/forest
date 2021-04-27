// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use super::{from_leb_bytes, to_leb_bytes, Error, Protocol, BLS_PUB_LEN, PAYLOAD_HASH_LEN};
use std::convert::TryInto;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::ops::Deref;
use std::u64;

/// Public key struct used as BLS Address data.
/// This type is only needed to be able to implement traits on it due to limitations on
/// arrays within Rust that are greater than 32 length. Can be dereferenced into `[u8; 48]`.
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

/// Payload is the data of the Address. Variants are the supported Address protocols.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq)]
pub enum Payload {
    /// ID protocol address.
    ID(u64),
    /// SECP256K1 key address, 20 byte hash of PublicKey
    Secp256k1([u8; PAYLOAD_HASH_LEN]),
    /// Actor protocol address, 20 byte hash of actor data
    Actor([u8; PAYLOAD_HASH_LEN]),
    /// BLS key address, full 48 byte public key
    BLS(BLSPublicKey),
}

impl Payload {
    /// Returns encoded bytes of Address without the protocol byte.
    pub fn to_raw_bytes(self) -> Vec<u8> {
        use Payload::*;
        match self {
            ID(i) => to_leb_bytes(i).unwrap(),
            Secp256k1(arr) => arr.to_vec(),
            Actor(arr) => arr.to_vec(),
            BLS(arr) => arr.to_vec(),
        }
    }

    /// Returns encoded bytes of Address including the protocol byte.
    pub(crate) fn to_bytes(self) -> Vec<u8> {
        use Payload::*;
        let mut bz = match self {
            ID(i) => to_leb_bytes(i).unwrap(),
            Secp256k1(arr) => arr.to_vec(),
            Actor(arr) => arr.to_vec(),
            BLS(arr) => arr.to_vec(),
        };

        bz.insert(0, Protocol::from(self) as u8);
        bz
    }

    /// Generates payload from raw bytes and protocol.
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

impl From<&Payload> for Protocol {
    fn from(pl: &Payload) -> Self {
        match pl {
            Payload::ID(_) => Self::ID,
            Payload::Secp256k1(_) => Self::Secp256k1,
            Payload::Actor(_) => Self::Actor,
            Payload::BLS(_) => Self::BLS,
        }
    }
}
