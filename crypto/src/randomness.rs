// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

/// Specifies a domain for randomness generation.
#[derive(PartialEq, Eq, Copy, Clone, FromPrimitive, Debug, Hash)]
#[repr(i64)]
pub enum DomainSeparationTag {
    TicketProduction = 1,
    ElectionPoStChallengeSeed = 2,
    WindowedPoStChallengeSeed = 3,
    SealRandomness = 4,
    InteractiveSealChallengeSeed = 5,
}

impl DomainSeparationTag {
    /// from_byte allows generating DST from encoded byte
    pub fn from_byte(b: u8) -> Option<Self> {
        FromPrimitive::from_u8(b)
    }
}
