// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::fmt::Display;
use std::ops::{Add, Div, Mul, Sub};
use fvm_shared::clock::ChainEpoch as ChainEpochV2;
use fvm_shared::clock::EPOCH_DURATION_SECONDS;
use serde::{Deserialize, Serialize};

pub const SECONDS_IN_DAY: i64 = 86400;
pub const EPOCHS_IN_DAY: i64 = SECONDS_IN_DAY / EPOCH_DURATION_SECONDS;

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ChainEpoch(pub ChainEpochV2);

impl From<ChainEpoch> for ChainEpochV2 {
    fn from(epoch: ChainEpoch) -> Self {
        epoch.0
    }
}

impl From<ChainEpoch> for u64 {
    fn from(epoch: ChainEpoch) -> Self {
        epoch.0 as u64
    }
}

impl quickcheck::Arbitrary for ChainEpoch {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        ChainEpoch(ChainEpochV2::arbitrary(g))
    }
}

impl Display for ChainEpoch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Mul<i64> for ChainEpoch {
    type Output = ChainEpoch;
    fn mul(self, rhs: i64) -> Self::Output {
        (self.0).mul(rhs).into()
    }
}

impl Mul<ChainEpoch> for ChainEpoch {
    type Output = ChainEpoch;
    fn mul(self, rhs: ChainEpoch) -> Self::Output {
        (self.0).mul(rhs.0).into()
    }
}

impl Div<ChainEpoch> for ChainEpoch {
    type Output = ChainEpoch;
    fn div(self, rhs: ChainEpoch) -> Self::Output {
        (self.0).div(rhs.0).into()
    }
}

impl Sub<ChainEpoch> for ChainEpoch {
    type Output = ChainEpoch;
    fn sub(self, rhs: ChainEpoch) -> Self::Output {
        (&self.0).sub(rhs.0).into()
    }
}

impl Sub<i64> for ChainEpoch {
    type Output = ChainEpoch;
    fn sub(self, rhs: i64) -> Self::Output {
        (&self.0).sub(rhs).into()
    }
}

impl Add<i64> for ChainEpoch {
    type Output = ChainEpoch;
    fn add(self, rhs: i64) -> Self::Output {
        (&self.0).add(rhs).into()
    }
}

impl From<i64> for ChainEpoch {
    fn from(epoch: i64) -> Self {
        ChainEpoch(epoch)
    }
}
