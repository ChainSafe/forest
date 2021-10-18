// Copyright 2019-2022 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::borrow::Cow;

/// Configuration parameters for the GraphSync protocol.
#[derive(Clone)]
pub struct GraphSyncConfig {
    /// The protocol id to negotiate this protocol (default is `/ipfs/graphsync/1.0.0`).
    pub protocol_id: Cow<'static, [u8]>,

    /// The maximum byte size for messages sent over the network.
    pub max_transmit_size: usize,
}

impl Default for GraphSyncConfig {
    fn default() -> Self {
        Self {
            protocol_id: Cow::Borrowed(b"/ipfs/graphsync/1.0.0"),
            max_transmit_size: 2048,
        }
    }
}
