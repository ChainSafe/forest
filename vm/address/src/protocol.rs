// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use std::fmt;
use std::hash::Hash;
use std::u64;

/// Protocol defines the addressing protocol used to derive data to an address
#[derive(PartialEq, Eq, Copy, Clone, FromPrimitive, Debug, Hash)]
#[repr(u8)]
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
