// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt::{Display, Formatter, Result as FmtResult};

use serde::Serialize;

// masks
const MINOR_MASK: u32 = 0xffff00;
const MAJOR_ONLY_MASK: u32 = 0xff0000;
const MINOR_ONLY_MASK: u32 = 0x00ff00;
const PATCH_ONLY_MASK: u32 = 0x0000ff;

/// Represents the current version of the API.
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct APIVersion {
    pub version: String,
    pub api_version: Version,
    pub block_delay: u64,
}

/// Integer based value on version information. Highest order bits for Major, Mid order for Minor
/// and lowest for Patch.
#[derive(Serialize)]
pub struct Version(u32);

impl Version {
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self(major << 16 | minor << 8 | patch)
    }

    fn ints(&self) -> (u32, u32, u32) {
        let v = self.0;
        (
            (v & MAJOR_ONLY_MASK) >> 16,
            (v & MINOR_ONLY_MASK) >> 8,
            v & PATCH_ONLY_MASK,
        )
    }
}

impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.0 & MINOR_MASK == other.0 & MINOR_MASK
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        let (major, minor, patch) = self.ints();
        write!(f, "{}.{}.{}", major, minor, patch)
    }
}
