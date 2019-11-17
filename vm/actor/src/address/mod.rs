mod test;
mod protocol;
pub use self::protocol::{Protocol};

use blake2::digest::{Input, VariableOutput};
use blake2::VarBlake2b;
use data_encoding::Encoding;
use data_encoding_macro::{internal_new_encoding, new_encoding};
use leb128;

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
const UNDEFINED_ADDR_STR: &str = "<empty>";

pub enum Network {
    Mainnet,
    Testnet,
}

impl Network {
    fn to_prefix(&self) -> &'static str {
        match self {
            Network::Mainnet => MAINNET_PREFIX,
            Network::Testnet => TESTNET_PREFIX,
        }
    }
}

#[derive(PartialEq)]
pub struct Address {
    protocol: Protocol,
    payload: Vec<u8>,
}

impl Address {
    fn new(protocol: Protocol, payload: Vec<u8>) -> Result<Self, String> {
        match protocol {
            // No validation needed for ID protocol
            Protocol::ID => (),
            Protocol::Secp256k1 | Protocol::Actor => {
                if payload.len() != PAYLOAD_HASH_LEN {
                    return Err(format!(
                        "Invalid payload length, wanted: {} got: {}",
                        PAYLOAD_HASH_LEN,
                        payload.len()
                    ));
                }
            }
            Protocol::BLS => {
                if payload.len() != BLS_PUB_LEN {
                    return Err(format!(
                        "Invalid BLS key length, wanted: {} got: {}",
                        BLS_PUB_LEN,
                        payload.len()
                    ));
                }
            }
            _ => return Err("unknown protocol".to_owned()),
        }

        // Create validated address
        Ok(Self { protocol, payload })
    }
    /// Creates address from formatted string
    pub fn from_bytes(bz: Vec<u8>) -> Result<Self, String> {
        if bz.is_empty() {
            Address::new(Protocol::Undefined, Vec::new())
        } else if bz.len() == 1 {
            Err("invalid byte length".to_owned())
        } else {
            let mut copy = bz.clone();
            let protocol = Protocol::from_byte(copy.remove(0));
            Address::new(protocol, copy)
        }
    }
    /// Creates address from formatted string
    pub fn from_string(addr: String) -> Result<Self, String> {
        if addr.is_empty() || addr == UNDEFINED_ADDR_STR {
            return Address::new(Protocol::Undefined, Vec::new());
        }
        if addr.len() > MAX_ADDRESS_LEN || addr.len() < 3 {
            return Err("invalid address length".to_owned());
        }
        if &addr[0..1] != MAINNET_PREFIX && &addr[0..1] != TESTNET_PREFIX {
            return Err(format!("unknown network prefix: {}", &addr[0..1]));
        }

        let protocol: Protocol = match &addr[1..2] {
            "0" => Protocol::ID,
            "1" => Protocol::Secp256k1,
            "2" => Protocol::Actor,
            "3" => Protocol::BLS,
            _ => Protocol::Undefined,
        };

        if protocol == Protocol::Undefined {
            return Err("unknown protocol".to_owned());
        }

        let raw = &addr[2..];
        if protocol == Protocol::ID {
            if raw.len() > 20 {
                // 20 is max u64 as string
                return Err("invalid payload length".to_owned());
            }
            let i = raw.parse::<u64>();
            if i.is_err() {
                return Err("could not parse payload string".to_owned());
            }

            return Address::new_id(i.unwrap());
        }

        let enc_res = ADDRESS_ENCODER.decode(raw.as_bytes());
        if let Err(e) = enc_res {
            return Err(format!("could not decode the address: {}", e));
        }

        let mut payload = enc_res.unwrap();
        let cksm = payload.split_off(payload.len() - CHECKSUM_HASH_LEN);

        if (protocol == Protocol::Secp256k1 || protocol == Protocol::Actor)
            && payload.len() != PAYLOAD_HASH_LEN
        {
            return Err("invalid payload".to_owned());
        }

        let mut ingest = payload.clone();
        ingest.insert(0, protocol as u8);
        if !validate_checksum(ingest, cksm) {
            return Err("invalid checksum".to_owned());
        }

        Address::new(protocol, payload)
    }

    /// Generates new address using ID protocol
    pub fn new_id(id: u64) -> Result<Self, String> {
        let mut buf = [0; 1023];

        // write id to buffer in leb128 format
        let mut writable = &mut buf[..];
        leb128::write::unsigned(&mut writable, id).expect("Should write number");

        // Create byte vector from buffer
        let vec = Vec::from(&buf[..]);
        Address::new(Protocol::ID, vec)
    }
    /// Generates new address using Secp256k1 pubkey
    pub fn new_secp256k1(pubkey: Vec<u8>) -> Result<Self, String> {
        Address::new(Protocol::Secp256k1, address_hash(pubkey))
    }
    /// Generates new address using Secp256k1 pubkey
    pub fn new_actor(data: Vec<u8>) -> Result<Self, String> {
        Address::new(Protocol::Actor, address_hash(data))
    }
    /// Generates new address using Secp256k1 pubkey
    pub fn new_bls(pubkey: Vec<u8>) -> Result<Self, String> {
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
    pub fn to_string(&self, network: Option<Network>) -> Result<String, String> {
        match self.protocol {
            Protocol::Undefined => Ok(UNDEFINED_ADDR_STR.to_owned()),
            Protocol::Secp256k1 | Protocol::Actor | Protocol::BLS => {
                let mut ingest = self.payload();
                ingest.insert(0, self.protocol() as u8);
                let cksm = checksum(ingest);
                let mut bz = self.payload();

                // payload bytes followed by calculated checksum
                bz.extend(cksm.clone());
                Ok(format!(
                    "{}{}{}",
                    match network {
                        Some(x) => x.to_prefix(),
                        None => Network::Testnet.to_prefix(),
                    },
                    self.protocol().to_string(),
                    ADDRESS_ENCODER.encode(bz.as_mut()),
                ))
            }
            Protocol::ID => {
                let mut buf = [0; 1023];
                buf.copy_from_slice(&self.payload());
                let mut readable = &buf[..];
                Ok(format!(
                    "{}{}{}",
                    match network {
                        Some(x) => x.to_prefix(),
                        None => Network::Testnet.to_prefix(),
                    },
                    self.protocol().to_string(),
                    leb128::read::unsigned(&mut readable).expect("should read encoded bytes"),
                ))
            }
        }
    }
    /// Returns if Address is empty
    pub fn empty(&self) -> bool {
        self.protocol == Protocol::Undefined
    }

    // Marshalling and unmarshalling
    pub fn unmarshall_cbor(&mut self, _bz: &mut [u8]) -> Result<(), String> {
        // TODO
        Err("Unmarshall is unimplemented".to_owned())
    }
    pub fn marshall_cbor(&self) -> Result<Vec<u8>, String> {
        // TODO
        Err("Marshall is unimplemented".to_owned())
    }

    // JSON Marshalling and unmarshalling
    pub fn unmarshall_json(&mut self, _bz: &mut [u8]) -> Result<(), String> {
        // TODO
        Err("JSON unmarshall is unimplemented".to_owned())
    }
    pub fn marshall_json(&self) -> Result<Vec<u8>, String> {
        // TODO
        Err("JSON marshall is unimplemented".to_owned())
    }
}

/// Checksum calculates the 4 byte checksum hash
pub fn checksum(ingest: Vec<u8>) -> Vec<u8> {
    hash(ingest, CHECKSUM_HASH_LEN)
}

/// Validates the checksum against the ingest data
pub fn validate_checksum(ingest: Vec<u8>, expect: Vec<u8>) -> bool {
    let digest = checksum(ingest);
    digest == expect
}

/// Returns an address hash for given data
fn address_hash(ingest: Vec<u8>) -> Vec<u8> {
    hash(ingest, PAYLOAD_HASH_LEN)
}

/// generates blake2b hash with provided size
fn hash(ingest: Vec<u8>, size: usize) -> Vec<u8> {
    let mut hasher = VarBlake2b::new(size).unwrap();
    hasher.input(ingest);

    // allocate hash result vector
    let mut result: Vec<u8> = vec![0; size];

    hasher.variable_result(|res| {
        // Copy result slice to vector return
        result[..size].clone_from_slice(res);
    });

    result
}
