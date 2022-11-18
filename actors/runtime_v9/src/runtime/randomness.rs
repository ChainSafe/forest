// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use num_derive::FromPrimitive;
use serde_repr::*;

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
