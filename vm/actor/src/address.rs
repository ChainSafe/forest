use std::cmp::PartialEq;
// use data_encoding::{Encoding};
// use data_encoding_macro::*;

// const ACCOUNT_ENCODER: Encoding = new_encoding!{
//     symbols: "abcdefghijklmnopqrstuvwxyz234567",
//     padding: None,
// };

#[derive(PartialEq, Copy, Clone)]
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

pub enum Network {
    Mainnet,
    Testnet,
}

pub struct Address {
    protocol: Protocol,
    payload: Vec<u8>,
}

impl Address {
    fn new(protocol: Protocol, payload: Vec<u8>) -> Self {
        Self {
            protocol: protocol,
            payload: payload,
        }
    }
    pub fn from_string(_addr: String) -> Self {
        Address::new(Protocol::Undefined, Vec::new())
    }

    /// Returns protocol for Address
    pub fn protocol(&self) -> Protocol {
        self.protocol
    }
    /// Returns data payload of Address
    pub fn payload(&self) -> Vec<u8> {
        self.payload.clone()
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
    fn test_protocol_version() {
        let new_addr = Address::new(Protocol::BLS, Vec::new());
        assert!(new_addr.protocol() == Protocol::BLS);
        assert!(new_addr.protocol() != Protocol::Undefined);
    }
    
    #[test]
    fn test_payload() {
        let data = vec![0, 1, 2];
        let new_addr = Address::new(Protocol::Undefined, data.clone());
        assert_eq!(new_addr.payload(), data);
    }
}
