// Copyright 2020 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use encoding::repr::*;
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;

/// Specifies a domain for randomness generation.
#[derive(PartialEq, Eq, Copy, Clone, FromPrimitive, Debug, Hash, Deserialize_repr)]
#[repr(i64)]
pub enum DomainSeparationTag {
    TicketProduction = 1,
    ElectionProofProduction = 2,
    WinningPoStChallengeSeed = 3,
    WindowedPoStChallengeSeed = 4,
    SealRandomness = 5,
    InteractiveSealChallengeSeed = 6,
    WindowPoStDeadlineAssignment = 7,
    MarketDealCronSeed = 8,
    PoStChainCommit = 9,
}

impl DomainSeparationTag {
    /// from_byte allows generating DST from encoded byte
    pub fn from_byte(b: u8) -> Option<Self> {
        FromPrimitive::from_u8(b)
    }
}
