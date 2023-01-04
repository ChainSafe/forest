// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use serde::{Deserialize, Serialize};

/// `ParityDb` configuration exposed in Forest.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ParityDbConfig {
    pub stats: bool,
    pub compression: String,
}

impl Default for ParityDbConfig {
    fn default() -> Self {
        Self {
            stats: false,
            compression: "lz4".into(),
        }
    }
}
