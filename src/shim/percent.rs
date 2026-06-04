// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use serde::{Deserialize, Serialize};

/// Lotus [`types.Percent`](https://github.com/filecoin-project/lotus/blob/master/chain/types/percent.go):
/// hundredths in memory (`125` = 25% over base).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    derive_more::Deref,
    derive_more::From,
)]
#[serde(transparent)]
pub struct Percent(pub u64);
