// use data_encoding::Encoding;
// use data_encoding_macro::{internal_new_encoding, new_encoding};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

// const ACCOUNT_ENCODER: Encoding = new_encoding! {
//     symbols: "abcdefghijklmnopqrstuvwxyz234567",
//     padding: None,
// };

#[derive(PartialEq, Copy, Clone, FromPrimitive)]
pub enum Protocol {
    // ID protocol addressing
    ID = 0,
    // SECP256K1 key addressing
    Secp256k1 = 1,
    // Actor protocol addressing
    Actor = 2,
    // BLS key addressing
    BLS = 3,

    Undefined = 255,
}

impl Protocol {
    fn from_byte(b: u8) -> Protocol {
        match FromPrimitive::from_u8(b) {
            Some(Protocol::ID) => Protocol::ID,
            Some(Protocol::Secp256k1) => Protocol::Secp256k1,
            Some(Protocol::Actor) => Protocol::Actor,
            Some(Protocol::BLS) => Protocol::BLS,
            _ => Protocol::Undefined,
        }
    }
}

pub enum Network {
    Mainnet,
    Testnet,
}

#[derive(PartialEq)]
pub struct Address {
    protocol: Protocol,
    payload: Vec<u8>,
}

impl Address {
    fn new(protocol: Protocol, payload: Vec<u8>) -> Result<Self, String> {
        // TODO: Check payload with protocol
        Ok(Self {
            protocol: protocol,
            payload: payload,
        })
    }
    /// Creates address from formatted string
    pub fn from_bytes(bz: Vec<u8>) -> Result<Self, String> {
        if bz.len() == 0 {
            Address::new(Protocol::Undefined, Vec::new())
        } else if bz.len() == 1 {
            Err(String::from("Invalid byte length"))
        } else {
            let mut copy = bz.clone();
            let protocol = Protocol::from_byte(copy.remove(0));
            Address::new(protocol, copy)
        }
    }
    /// Creates address from formatted string
    pub fn from_string(_addr: String) -> Result<Self, String> {
        // TODO
        Address::new(Protocol::Undefined, Vec::new())
    }

    /// Generates new address using ID protocol
    pub fn new_id(_id: u64) -> Result<Self, String> {
        // TODO: implement leb128 from u64 for bz
        Address::new(Protocol::ID, Vec::new())
    }
    /// Generates new address using Secp256k1 pubkey
    pub fn new_secp256k1(pubkey: Vec<u8>) -> Result<Self, String> {
        // TODO address hash to payload
        Address::new(Protocol::Secp256k1, pubkey)
    }
    /// Generates new address using Secp256k1 pubkey
    pub fn new_actor(data: Vec<u8>) -> Result<Self, String> {
        // TODO address hash to payload
        Address::new(Protocol::BLS, data)
    }
    /// Generates new address using Secp256k1 pubkey
    pub fn new_bls(pubkey: Vec<u8>) -> Result<Self, String> {
        Address::new(Protocol::ID, pubkey)
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
    pub fn to_string(&self) -> String {
        // TODO: implement
        String::from("")
    }
    /// Returns if Address is empty
    pub fn empty(&self) -> bool {
        self.protocol == Protocol::Undefined
    }

    // Marshalling and unmarshalling
    pub fn unmarshall_cbor(&mut self, _bz: &mut [u8]) -> Result<(), String> {
        // TODO: implement
        Err(String::from("Unmarshall is unimplemented"))
    }
    pub fn marshall_cbor(&self) -> Result<Vec<u8>, String> {
        // TODO: implement
        Err(String::from("Marshall is unimplemented"))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn protocol_version() {
        let new_addr = Address::new(Protocol::BLS, Vec::new()).unwrap();
        assert!(new_addr.protocol() == Protocol::BLS);
        assert!(new_addr.protocol() != Protocol::Undefined);
    }

    #[test]
    fn payload() {
        let data = vec![0, 1, 2];
        let new_addr = Address::new(Protocol::Undefined, data.clone()).unwrap();
        assert_eq!(new_addr.payload(), data);
    }

    #[test]
    fn bytes() {
        let data = vec![0, 3, 2];
        let new_addr = Address::new(Protocol::Secp256k1, data.clone()).unwrap();
        let encoded_bz = new_addr.to_bytes();
        assert_eq!(encoded_bz, vec![Protocol::Secp256k1 as u8, 0, 3, 2]);

        // Assert decoded address equals the original address and a new one with the same data
        let decoded_addr = Address::from_bytes(encoded_bz).unwrap();
        assert!(decoded_addr == new_addr);
        assert!(decoded_addr == Address::new(Protocol::Secp256k1, data.clone()).unwrap());

        // Assert different types don't match
        assert!(decoded_addr != Address::new(Protocol::BLS, data.clone()).unwrap());
        assert!(decoded_addr != Address::new(Protocol::Secp256k1, vec![1, 2, 1]).unwrap());
    }
}
