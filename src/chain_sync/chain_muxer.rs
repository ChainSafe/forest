// Copyright 2019-2026 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use serde::{Deserialize, Serialize};

pub const DEFAULT_RECENT_STATE_ROOTS: i64 = 2000;

/// Structure that defines syncing configuration options
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub struct SyncConfig {
    /// Number of recent state roots to keep in the database after `sync`
    /// and to include in the exported snapshot.
    pub recent_state_roots: i64,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            recent_state_roots: DEFAULT_RECENT_STATE_ROOTS,
        }
    }
}
