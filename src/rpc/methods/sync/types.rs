// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

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

#[derive(Default, Clone)]
pub struct SnapshotProgressTracker(Arc<parking_lot::RwLock<SnapshotProgressState>>);

impl SnapshotProgressTracker {
    /// Initializes the snapshot progress tracker and returns a callback function that updates the tracker
    pub fn create_callback(&self) -> Option<Arc<dyn Fn(String) + Send + Sync>> {
        let snapshot_progress_tracker = self.0.clone();

        // Set the snapshot progress tracker to in progress state only
        // when the callback is created (snapshot download starts)
        {
            let mut tracker = snapshot_progress_tracker.write();
            *tracker = SnapshotProgressState::InProgress {
                message: "Loading progress...".to_string(),
            };
        }

        Some(Arc::new(move |msg: String| {
            snapshot_progress_tracker
                .write()
                .set_in_progress(msg.clone());
        }))
    }

    /// Resets the snapshot progress tracker, once the snapshot download is finished
    pub fn reset(&self) {
        self.0.write().set_completed();
    }

    pub fn state(&self) -> SnapshotProgressState {
        self.0.read().clone()
    }
}
