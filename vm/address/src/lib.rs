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

/// Address is the struct that defines the protocol and data payload conversion from either
/// a public key or value
#[derive(PartialEq, Clone, Debug)]
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
            let mut copy = bz.clone();
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
        let mut buf = [0; 1023];

        // write id to buffer in leb128 format
        let mut writable = &mut buf[..];
        leb128::write::unsigned(&mut writable, id).expect("Should write number");

        // Create byte vector from buffer
        let vec = Vec::from(&buf[..]);
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
    fn unmarshal_cbor(_bz: &[u8]) -> Result<Self, EncodingError> {
        // TODO
        Err(EncodingError::Unmarshalling {
            description: "Not Implemented".to_string(),
            protocol: CodecProtocol::Cbor,
        })
    }
    fn marshal_cbor(&self) -> Result<Vec<u8>, EncodingError> {
        // TODO
        Err(EncodingError::Marshalling {
            description: format!("Not implemented, data: {:?}", self),
            protocol: CodecProtocol::Cbor,
        })
    }
}

/// encode converts the address into a string
fn encode(addr: &Address, network: Network) -> String {
    match addr.protocol {
        Protocol::Secp256k1 | Protocol::Actor | Protocol::BLS => {
            let mut ingest = addr.payload();
            ingest.insert(0, addr.protocol() as u8);
            let cksm = checksum(ingest);
            let mut bz = addr.payload();

            // payload bytes followed by calculated checksum
            bz.extend(cksm.clone());
            format!(
                "{}{}{}",
                network.to_prefix(),
                addr.protocol().to_string(),
                ADDRESS_ENCODER.encode(bz.as_mut()),
            )
        }
        Protocol::ID => {
            let mut buf = [0; 1023];
            buf.copy_from_slice(&addr.payload());
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
