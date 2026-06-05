// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use serde::{Deserialize, Serialize};

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
/// See <https://github.com/filecoin-project/lotus/blob/abe268e4011a7695cd270ee3ae988b63104fb79e/chain/types/percent.go>
pub struct Percent(pub u64);
