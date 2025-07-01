// Copyright 2019-2025 ChainSafe Systems
// SPDX-License-Identifier: Apache-2.0, MIT

use std::sync::Arc;

use crate::lotus_json::lotus_json_with_self;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "PascalCase")]
pub enum SnapshotProgressState {
    Initializing,
    InProgress { message: String },
    Completed,
    NotRequired,
}

impl SnapshotProgressState {
    pub fn set_in_progress(&mut self, message: String) {
        *self = Self::InProgress { message };
    }

    pub fn set_completed(&mut self) {
        *self = Self::Completed;
    }

    pub fn not_required(&mut self) {
        *self = Self::NotRequired;
    }

    pub fn is_completed(&self) -> bool {
        matches!(self, Self::Completed)
    }

    pub fn is_not_required(&self) -> bool {
        matches!(self, Self::NotRequired)
    }
}

impl Default for SnapshotProgressState {
    fn default() -> Self {
        Self::Initializing
    }
}

impl std::fmt::Display for SnapshotProgressState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Initializing => {
                write!(f, "🔄 Initializing (Checking if snapshot is needed)")
            }
            Self::InProgress { message } => {
                write!(f, "🌳 In Progress: {message}")
            }
            Self::Completed => {
                write!(f, "✅ Recently Completed! Chain will start syncing shortly")
            }
            Self::NotRequired => {
                write!(f, "⏳ Not Required (Snapshot is not needed)")
            }
        }
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

    /// Sets the snapshot progress state to completed, once the snapshot download is finished
    pub fn completed(&self) {
        self.0.write().set_completed();
    }

    /// Sets the snapshot progress state to not required, if downloading the snapshot is not required
    pub fn not_required(&self) {
        self.0.write().not_required();
    }

    /// Returns true if the snapshot progress state is completed
    pub fn is_completed(&self) -> bool {
        self.0.read().is_completed()
    }

    /// Returns the current snapshot progress state
    pub fn state(&self) -> SnapshotProgressState {
        self.0.read().clone()
    }
}
