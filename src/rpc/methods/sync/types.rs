// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT
// Add to src/rpc/types.rs or a similar appropriate location

use crate::lotus_json::lotus_json_with_self;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub struct SnapshotProgressTracker {
    pub message: String, // The formatted progress message
}

impl SnapshotProgressTracker {
    pub fn new() -> SnapshotProgressTracker {
        Self {
            message: String::new(),
        }
    }

    pub fn set_message(&mut self, message: String) {
        self.message = message;
    }
}

lotus_json_with_self!(SnapshotProgressTracker);
