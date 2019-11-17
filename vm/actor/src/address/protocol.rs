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
}

impl Protocol {
    pub(crate) fn from_byte(b: u8) -> Option<Protocol> {
        FromPrimitive::from_u8(b)
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let i = *self as u8;
        write!(f, "{}", i)
    }
}
