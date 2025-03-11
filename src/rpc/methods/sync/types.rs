// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use crate::lotus_json::lotus_json_with_self;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum SnapshotProgressState {
    InProgress { message: String },
    Completed,
    NotStarted,
}

impl SnapshotProgressState {
    pub fn set_in_progress(&mut self, message: String) {
        *self = Self::InProgress { message };
    }

    pub fn set_completed(&mut self) {
        *self = Self::Completed;
    }
}

impl Default for SnapshotProgressState {
    fn default() -> Self {
        Self::NotStarted
    }
}

lotus_json_with_self!(SnapshotProgressState);
