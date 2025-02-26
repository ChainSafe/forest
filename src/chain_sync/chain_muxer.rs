// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use serde::{Deserialize, Serialize};

// Sync the messages for one or many tipsets @ a time
// Lotus uses a window size of 8: https://github.com/filecoin-project/lotus/blob/c1d22d8b3298fdce573107413729be608e72187d/chain/sync.go#L56
const DEFAULT_REQUEST_WINDOW: usize = 8;
const DEFAULT_TIPSET_SAMPLE_SIZE: usize = 1;
const DEFAULT_RECENT_STATE_ROOTS: i64 = 2000;

/// Structure that defines syncing configuration options
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(derive_quickcheck_arbitrary::Arbitrary))]
pub struct SyncConfig {
    /// Request window length for tipsets during chain exchange
    #[cfg_attr(test, arbitrary(gen(|g| u32::arbitrary(g) as _)))]
    pub request_window: usize,
    /// Number of recent state roots to keep in the database after `sync`
    /// and to include in the exported snapshot.
    pub recent_state_roots: i64,
    /// Sample size of tipsets to acquire before determining what the network
    /// head is
    #[cfg_attr(test, arbitrary(gen(|g| u32::arbitrary(g) as _)))]
    pub tipset_sample_size: usize,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            request_window: DEFAULT_REQUEST_WINDOW,
            recent_state_roots: DEFAULT_RECENT_STATE_ROOTS,
            tipset_sample_size: DEFAULT_TIPSET_SAMPLE_SIZE,
        }
    }
}
