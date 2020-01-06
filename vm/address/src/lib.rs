// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

mod errors;
mod network;
mod protocol;
pub use self::errors::Error;
pub use self::network::Network;
pub use self::protocol::Protocol;

use data_encoding::Encoding;
use data_encoding_macro::{internal_new_encoding, new_encoding};
use encoding::{blake2b_variable, Cbor, CodecProtocol, Error as EncodingError};
use leb128;
use serde_cbor::Value::Bytes;
use serde_cbor::{from_slice, to_vec};
use std::hash::Hash;

/// defines the encoder for base32 encoding with the provided string with no padding
const ADDRESS_ENCODER: Encoding = new_encoding! {
    symbols: "abcdefghijklmnopqrstuvwxyz234567",
    padding: None,
};

pub const BLS_PUB_LEN: usize = 48;
pub const PAYLOAD_HASH_LEN: usize = 20;
pub const CHECKSUM_HASH_LEN: usize = 4;
const MAX_ADDRESS_LEN: usize = 84 + 2;
const MAINNET_PREFIX: &str = "f";
const TESTNET_PREFIX: &str = "t";
const BUFFER_SIZE: usize = 1024;

/// Address is the struct that defines the protocol and data payload conversion from either
/// a public key or value
#[derive(PartialEq, Eq, Clone, Debug, Hash)]
pub struct Address {
    protocol: Protocol,
    payload: Vec<u8>,
}

impl Address {
    /// Address constructor
    fn new(protocol: Protocol, payload: Vec<u8>) -> Result<Self, Error> {
        // Validates the data satisfies the protocol specifications
        match protocol {
            Protocol::ID => (),
            Protocol::Secp256k1 | Protocol::Actor => {
                if payload.len() != PAYLOAD_HASH_LEN {
                    return Err(Error::InvalidPayloadLength(payload.len()));
                }
            }
            Protocol::BLS => {
                if payload.len() != BLS_PUB_LEN {
                    return Err(Error::InvalidBLSLength(payload.len()));
                }
            }
        }

        // Create validated address
        Ok(Self { protocol, payload })
    }
    /// Creates address from encoded bytes
    pub fn from_bytes(bz: Vec<u8>) -> Result<Self, Error> {
        if bz.len() < 2 {
            Err(Error::InvalidLength)
        } else {
            let mut copy = bz;
            let protocol = Protocol::from_byte(copy.remove(0)).ok_or(Error::UnknownProtocol)?;
            Address::new(protocol, copy)
        }
    }
    /// Creates address from formatted string
    pub fn from_string(addr: String) -> Result<Self, Error> {
        if addr.len() > MAX_ADDRESS_LEN || addr.len() < 3 {
            return Err(Error::InvalidLength);
        }
        // ensure the network character is valid before converting
        if &addr[0..1] != MAINNET_PREFIX && &addr[0..1] != TESTNET_PREFIX {
            return Err(Error::UnknownNetwork);
        }

        // get protocol from second character
        let protocol: Protocol = match &addr[1..2] {
            "0" => Protocol::ID,
            "1" => Protocol::Secp256k1,
            "2" => Protocol::Actor,
            "3" => Protocol::BLS,
            _ => {
                return Err(Error::UnknownProtocol);
            }
        };

        // bytes after the protocol character is the data payload of the address
        let raw = &addr[2..];
        if protocol == Protocol::ID {
            if raw.len() > 20 {
                // 20 is max u64 as string
                return Err(Error::InvalidLength);
            }
            let i = raw.parse::<u64>()?;
            return Address::new_id(i);
        }

        // decode using byte32 encoding
        let mut payload = ADDRESS_ENCODER.decode(raw.as_bytes())?;
        // payload includes checksum at end, so split after decoding
        let cksm = payload.split_off(payload.len() - CHECKSUM_HASH_LEN);

        // sanity check to make sure address hash values are correct length
        if (protocol == Protocol::Secp256k1 || protocol == Protocol::Actor)
            && payload.len() != PAYLOAD_HASH_LEN
        {
            return Err(Error::InvalidPayload);
        }

        // sanity check to make sure bls pub key is correct length
        if protocol == Protocol::BLS && payload.len() != BLS_PUB_LEN {
            return Err(Error::InvalidPayload);
        }

        // validate checksum
        let mut ingest = payload.clone();
        ingest.insert(0, protocol as u8);
        if !validate_checksum(ingest, cksm) {
            return Err(Error::InvalidChecksum);
        }

        Address::new(protocol, payload)
    }

    /// Generates new address using ID protocol
    pub fn new_id(id: u64) -> Result<Self, Error> {
        let mut buf = [0; BUFFER_SIZE];

        // write id to buffer in leb128 format
        let mut writable = &mut buf[..];
        let size = leb128::write::unsigned(&mut writable, id)?;

        // Create byte vector from buffer
        let vec = Vec::from(&buf[..size]);
        Address::new(Protocol::ID, vec)
    }
    /// Generates new address using Secp256k1 pubkey
    pub fn new_secp256k1(pubkey: Vec<u8>) -> Result<Self, Error> {
        Address::new(Protocol::Secp256k1, address_hash(pubkey))
    }
    /// Generates new address using the Actor protocol
    pub fn new_actor(data: Vec<u8>) -> Result<Self, Error> {
        Address::new(Protocol::Actor, address_hash(data))
    }
    /// Generates new address using BLS pubkey
    pub fn new_bls(pubkey: Vec<u8>) -> Result<Self, Error> {
        Address::new(Protocol::BLS, pubkey)
    }

    /// Returns protocol for Address
    pub fn protocol(&self) -> Protocol {
        self.protocol
    }
    /// Returns data payload of Address
    pub fn payload(&self) -> Vec<u8> {
        self.payload.clone()
    }
    /// Returns encoded bytes of Address
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bz: Vec<u8> = self.payload();
        bz.insert(0, self.protocol() as u8);
        bz
    }
    /// Returns encoded string from Address
    pub fn to_string(&self, network: Option<Network>) -> String {
        match network {
            Some(net) => encode(self, net),
            None => encode(self, Network::Testnet),
        }
    }
}

impl Cbor for Address {
    fn unmarshal_cbor(bz: &[u8]) -> Result<Self, EncodingError> {
        // Convert cbor encoded to bytes
        let mut vec = match from_slice(bz) {
            Ok(Bytes(v)) => v,
            _ => {
                return Err(EncodingError::Unmarshalling {
                    description: "Could not decode as bytes".to_owned(),
                    protocol: CodecProtocol::Cbor,
                });
            }
        };
        // Remove protocol byte
        let protocol = Protocol::from_byte(vec.remove(0)).ok_or(EncodingError::Unmarshalling {
            description: format!("Invalid protocol byte: {}", bz[0]),
            protocol: CodecProtocol::Cbor,
        })?;
        // Create and return created address of unmarshalled bytes
        Ok(Address::new(protocol, vec)?)
    }
    fn marshal_cbor(&self) -> Result<Vec<u8>, EncodingError> {
        // encode bytes
        Ok(to_vec(&Bytes(self.to_bytes()))?)
    }
}

impl From<Error> for EncodingError {
    fn from(err: Error) -> EncodingError {
        EncodingError::Marshalling {
            description: err.to_string(),
            protocol: CodecProtocol::Cbor,
        }
    }
}

/// encode converts the address into a string
fn encode(addr: &Address, network: Network) -> String {
    match addr.protocol {
        Protocol::Secp256k1 | Protocol::Actor | Protocol::BLS => {
            let ingest = addr.to_bytes();
            let mut bz = addr.payload();

            // payload bytes followed by calculated checksum
            bz.extend(checksum(ingest));
            format!(
                "{}{}{}",
                network.to_prefix(),
                addr.protocol().to_string(),
                ADDRESS_ENCODER.encode(bz.as_mut()),
            )
        }
        Protocol::ID => {
            let mut buf = [0; BUFFER_SIZE];
            buf[..addr.payload().len()].copy_from_slice(&addr.payload());
            let mut readable = &buf[..];
            format!(
                "{}{}{}",
                network.to_prefix(),
                addr.protocol().to_string(),
                leb128::read::unsigned(&mut readable).expect("should read encoded bytes"),
            )
        }
    }
}

/// Checksum calculates the 4 byte checksum hash
pub fn checksum(ingest: Vec<u8>) -> Vec<u8> {
    blake2b_variable(ingest, CHECKSUM_HASH_LEN)
}

/// Validates the checksum against the ingest data
pub fn validate_checksum(ingest: Vec<u8>, expect: Vec<u8>) -> bool {
    let digest = checksum(ingest);
    digest == expect
}

/// Returns an address hash for given data
fn address_hash(ingest: Vec<u8>) -> Vec<u8> {
    blake2b_variable(ingest, PAYLOAD_HASH_LEN)
}
