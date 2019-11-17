use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use std::fmt;

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
    pub(crate) fn from_byte(b: u8) -> Protocol {
        match FromPrimitive::from_u8(b) {
            Some(Protocol::ID) => Protocol::ID,
            Some(Protocol::Secp256k1) => Protocol::Secp256k1,
            Some(Protocol::Actor) => Protocol::Actor,
            Some(Protocol::BLS) => Protocol::BLS,
            _ => Protocol::Undefined,
        }
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let i = *self as u8;
        write!(f, "{}", i)
    }
}
