// Copyright 2019-2023 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use serde::{Deserialize, Serialize};

/// `ParityDb` configuration exposed in Forest.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
#[serde(default)]
pub struct ParityDbConfig {
    pub enable_statistics: bool,
}
