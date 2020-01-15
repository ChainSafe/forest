// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0

use crate::Error;

/// Cid protocol version
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum Version {
    V0,
    V1,
}

use Version::*;

impl Version {
    /// Generates version from integer value
    pub fn from(raw: u64) -> Result<Version, Error> {
        match raw {
            0 => Ok(V0),
            1 => Ok(V1),
            _ => Err(Error::InvalidCidVersion),
        }
    }

    /// Returns true if string is version 0
    pub fn is_v0_str(data: &str) -> bool {
        // v0 is a base58btc encoded sha hash, so it has
        // fixed length and always begins with "Qm"
        data.len() == 46 && data.starts_with("Qm")
    }

    /// Returns true if bytes is version 0
    pub fn is_v0_binary(data: &[u8]) -> bool {
        data.len() == 34 && data.starts_with(&[0x12, 0x20])
    }
}

impl From<Version> for u64 {
    fn from(ver: Version) -> u64 {
        match ver {
            V0 => 0,
            V1 => 1,
        }
    }
}
